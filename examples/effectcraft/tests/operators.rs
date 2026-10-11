#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
mod support;
use effectcraft_keyframe::Value;
use effectcraft_project::{Node, Project, PropGroup};
use support::{field, group, project};

fn find<'a>(g: &'a PropGroup, name: &str) -> Option<&'a PropGroup> {
    if g.match_id == name {
        return Some(g);
    }
    g.children.iter().find_map(|n| match n {
        Node::Group(g) => find(g, name),
        _ => None,
    })
}
fn first<'a>(p: &'a Project, name: &str) -> Option<&'a PropGroup> {
    p.comps()
        .flat_map(|(_, c)| &c.layers)
        .find_map(|l| find(&l.props, name))
}
fn rect() -> Vec<u8> {
    group(
        "ADBE Vector Shape - Rect",
        vec![field("ADBE Vector Rect Size", &[100.0; 2])],
        true,
    )
}
fn op(kind: &str, name: &str, value: f64) -> Vec<u8> {
    group(
        &format!("ADBE Vector Filter - {kind}"),
        vec![field(name, &[value])],
        true,
    )
}

#[test]
fn static_modifiers_map_explicit_values_and_enums() {
    for (kind, name, input, target, path, expected) in [
        (
            "RC",
            "ADBE Vector RoundCorner Radius",
            23.0,
            "round",
            "radius",
            Value::Scalar(23.0),
        ),
        (
            "PB",
            "ADBE Vector PuckerBloat Amount",
            -40.0,
            "pucker",
            "amount",
            Value::Scalar(-40.0),
        ),
        (
            "Twist",
            "ADBE Vector Twist Angle",
            150.0,
            "twist",
            "angle",
            Value::Scalar(150.0),
        ),
        (
            "Zigzag",
            "ADBE Vector Zigzag Detail",
            5.0,
            "zigzag",
            "ridges",
            Value::Scalar(5.0),
        ),
        (
            "Offset",
            "ADBE Vector Offset Amount",
            -8.0,
            "offset",
            "amount",
            Value::Scalar(-8.0),
        ),
    ] {
        let r = aep_effectcraft::import(&project(vec![rect(), op(kind, name, input)])).unwrap();
        assert_eq!(
            first(&r.project, target).unwrap().prop(path).unwrap().value,
            expected
        );
        Project::from_json(&r.project.to_file_json().unwrap()).unwrap();
    }
    for i in 1..=5 {
        let r = aep_effectcraft::import(&project(vec![
            rect(),
            op("Merge", "ADBE Vector Merge Type", f64::from(i)),
        ]))
        .unwrap();
        assert_eq!(
            first(&r.project, "merge")
                .unwrap()
                .prop("mode")
                .unwrap()
                .value,
            Value::Enum(i - 1)
        );
    }
}

#[test]
fn invalid_duplicate_animated_and_expressed_modifiers_are_local_omissions() {
    let name = "ADBE Vector Twist Angle";
    let bad = vec![
        vec![field(name, &[f64::NAN])],
        vec![field(name, &[4000.0])],
        vec![field(name, &[30.0]), field(name, &[40.0])],
        vec![support::keyed_field(name, &[(0, 0.0), (30720, 4000.0)])],
        vec![support::field_with(
            name,
            &[30.0],
            vec![support::chunk(b"Utf8", b"time*30")],
        )],
    ];
    for fields in bad {
        let r = aep_effectcraft::import(&project(vec![
            rect(),
            group("ADBE Vector Filter - Twist", fields, true),
        ]))
        .unwrap();
        assert!(first(&r.project, "twist").is_none());
        assert!(first(&r.project, "rect").is_some());
        assert!(
            r.report
                .warnings
                .iter()
                .any(|w| w.message.contains("omitted"))
        );
    }
    for v in [0.0, 6.0, 2.5] {
        let r = aep_effectcraft::import(&project(vec![
            rect(),
            op("Merge", "ADBE Vector Merge Type", v),
        ]))
        .unwrap();
        assert!(first(&r.project, "merge").is_none());
    }
}

#[test]
fn expansion_budget_cannot_be_reset_by_nested_groups() {
    let mut nodes = vec![rect()];
    for _ in 0..4 {
        nodes = vec![
            group(
                "ADBE Vector Group",
                vec![group("ADBE Vectors Group", nodes, true)],
                true,
            ),
            op("Twist", "ADBE Vector Twist Angle", 180.0),
        ];
    }
    let r = aep_effectcraft::import(&project(nodes)).unwrap();
    assert!(first(&r.project, "twist").is_none());
    assert!(first(&r.project, "rect").is_some());
    assert!(
        r.report
            .warnings
            .iter()
            .any(|w| w.message.contains("expansion"))
    );
}

#[test]
fn offset_copy_semantic_conflict_is_reported() {
    let r = aep_effectcraft::import(&project(vec![
        rect(),
        group(
            "ADBE Vector Filter - Offset",
            vec![field("ADBE Vector Offset Copies", &[2.0])],
            true,
        ),
    ]))
    .unwrap();
    assert!(first(&r.project, "offset").is_none());
    assert!(
        r.report
            .warnings
            .iter()
            .any(|w| w.message.contains("semantics differ"))
    );
}

#[test]
fn styles_and_trim_mode_use_explicit_one_based_mapping() {
    let r = aep_effectcraft::import(&project(vec![
        rect(),
        group(
            "ADBE Vector Graphic - Stroke",
            vec![
                field("ADBE Vector Stroke Line Cap", &[2.0]),
                field("ADBE Vector Stroke Line Join", &[3.0]),
            ],
            true,
        ),
        group(
            "ADBE Vector Graphic - Fill",
            vec![field("ADBE Vector Fill Rule", &[2.0])],
            true,
        ),
        op("Trim", "ADBE Vector Trim Type", 2.0),
    ]))
    .unwrap();
    for (kind, name, expected) in [
        ("stroke", "cap", 1),
        ("stroke", "join", 2),
        ("fill", "rule", 1),
        ("trim", "mode", 1),
    ] {
        assert_eq!(
            first(&r.project, kind).unwrap().prop(name).unwrap().value,
            Value::Enum(expected)
        );
    }
}

#[test]
#[ignore = "local original LottieFiles AEP samples"]
fn native_modifiers_preserve_original_values() {
    let root = std::path::PathBuf::from(std::env::var_os("AEP_DATA_ROOT").unwrap())
        .join("samples/github-licensed/_extracted/LottieFiles_test-files-main/test-files-main/data/shape-modifiers");
    for (file, kind, name, expected) in [
        ("round-corners.aep", "round", "radius", 10.0),
        ("pucker.aep", "pucker", "amount", -40.0),
        ("zig-zag.aep", "zigzag", "ridges", 5.0),
        ("offset-path.aep", "offset", "amount", 8.0),
    ] {
        let r = aep_effectcraft::import(&std::fs::read(root.join(file)).unwrap()).unwrap();
        assert_eq!(
            first(&r.project, kind)
                .unwrap_or_else(|| panic!("{file}: {:?}", r.report))
                .prop(name)
                .unwrap()
                .value,
            Value::Scalar(expected),
            "{file}"
        );
    }
}

#[test]
fn operator_animation_preserves_timing_and_values() {
    let r = aep_effectcraft::import(&project(vec![
        rect(),
        group(
            "ADBE Vector Filter - Twist",
            vec![support::keyed_field(
                "ADBE Vector Twist Angle",
                &[(0, 0.0), (30720, 90.0)],
            )],
            true,
        ),
    ]))
    .unwrap();
    let p = first(&r.project, "twist").unwrap().prop("angle").unwrap();
    assert_eq!(p.keys.len(), 2);
    assert_eq!(
        p.keys[1].time,
        effectcraft_time::Tick::from_seconds_f64(1.0)
    );
    assert_eq!(p.keys[1].value, Value::Scalar(90.0));
}

#[cfg(feature = "render-check")]
#[test]
fn modifiers_change_pixels_and_disabled_modifiers_restore_the_control() {
    use effectcraft_time::Tick;
    struct NoMedia;
    impl effectcraft_render::FootageSource for NoMedia {
        fn frame(
            &self,
            _: effectcraft_project::ItemId,
            _: &effectcraft_project::Footage,
            _: Tick,
        ) -> Option<std::sync::Arc<effectcraft_render::Image>> {
            None
        }
    }
    fn render(bytes: &[u8]) -> Vec<u8> {
        let mut r = aep_effectcraft::import(bytes).unwrap();
        let id = *r.project.comps().next().unwrap().0;
        let l = &mut r.project.comp_mut(id).unwrap().layers[0];
        l.switches.video = true;
        l.in_point = Tick::ZERO;
        for (name, value) in [
            ("position", Value::Vec3([128.0, 128.0, 0.0])),
            ("anchor", Value::Vec3([0.0; 3])),
            ("scale", Value::Vec3([100.0; 3])),
            ("opacity", Value::Scalar(100.0)),
        ] {
            l.props
                .prop_mut(&format!("transform/{name}"))
                .unwrap()
                .value = value;
        }
        let img = effectcraft_render::Renderer::new(
            &r.project,
            &NoMedia,
            effectcraft_render::RenderOpts {
                motion_blur: false,
                ..Default::default()
            },
        )
        .comp_frame(id, Tick::ZERO);
        (0..256)
            .flat_map(|y| (0..256).map(move |x| (x, y)))
            .map(|(x, y)| (img.get(x, y)[3] * 255.0).round() as u8)
            .collect()
    }
    let fixture = |kind: &str, field_name: &str, value: f64, enabled: bool| {
        project(vec![
            rect(),
            group(
                "ADBE Vector Shape - Rect",
                vec![
                    field("ADBE Vector Rect Size", &[50.0; 2]),
                    field("ADBE Vector Rect Position", &[35.0, 0.0]),
                ],
                true,
            ),
            group(
                &format!("ADBE Vector Filter - {kind}"),
                vec![field(field_name, &[value])],
                enabled,
            ),
            group(
                "ADBE Vector Graphic - Fill",
                vec![field("ADBE Vector Fill Color", &[255.0; 4])],
                true,
            ),
        ])
    };
    let baseline = render(&fixture("RC", "ADBE Vector RoundCorner Radius", 0.0, false));
    assert!(baseline.iter().any(|&a| a > 0));
    for (kind, name, value) in [
        ("RC", "ADBE Vector RoundCorner Radius", 20.0),
        ("PB", "ADBE Vector PuckerBloat Amount", -40.0),
        ("Twist", "ADBE Vector Twist Angle", 120.0),
        ("Zigzag", "ADBE Vector Zigzag Detail", 5.0),
        ("Offset", "ADBE Vector Offset Amount", 8.0),
        ("Merge", "ADBE Vector Merge Type", 3.0),
    ] {
        assert_ne!(
            render(&fixture(kind, name, value, true)),
            baseline,
            "{kind} must alter the contour"
        );
        assert_eq!(
            render(&fixture(kind, name, value, false)),
            baseline,
            "disabled {kind}"
        );
    }
}
