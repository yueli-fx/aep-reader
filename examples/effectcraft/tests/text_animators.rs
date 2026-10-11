#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
mod support;
use effectcraft_keyframe::Value;
use effectcraft_project::{Node, Project, PropGroup};
use effectcraft_time::Tick;
use support::{chunk, field, group, list, project_properties};

fn range(fields: Vec<Vec<u8>>, advanced: Vec<Vec<u8>>) -> Vec<u8> {
    let mut fields = fields;
    fields.push(group("ADBE Text Range Advanced", advanced, true));
    group("ADBE Text Selector", fields, true)
}
fn fixture(props: Vec<Vec<u8>>, selectors: Vec<Vec<u8>>) -> Vec<u8> {
    let mut text = chunk(b"tdmn", b"ADBE Text Document");
    text.extend(list(
        b"btds",
        vec![list(
            b"btdk",
            vec![b"/1 << /1 [ << /0 << /0 (ABCDEF) >> >> ] >>".to_vec()],
        )],
    ));
    project_properties(vec![
        text,
        group(
            "ADBE Text Animators",
            vec![group(
                "ADBE Text Animator",
                vec![
                    group("ADBE Text Selectors", selectors, true),
                    group("ADBE Text Animator Properties", props, true),
                ],
                true,
            )],
            true,
        ),
    ])
}
fn anim(p: &Project) -> Option<&PropGroup> {
    p.comps()
        .flat_map(|(_, c)| &c.layers)
        .find_map(|l| l.props.group("text/animators/animator"))
}
fn native(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::PathBuf::from(std::env::var_os("AEP_FIXTURE_ROOT").unwrap()).join(name),
    )
    .unwrap()
}
#[test]
fn multiple_channels_keep_vector_units_color_and_key_times() {
    let r = aep_effectcraft::import(&fixture(
        vec![
            field("ADBE Text Position 3D", &[100.0, 50.0, 0.0]),
            field("ADBE Text Scale 3D", &[200.0, 180.0, 100.0]),
            field("ADBE Text Fill Color", &[255.0, 10.0, 20.0, 30.0]),
            support::keyed_field("ADBE Text Opacity", &[(0, 0.0), (30720, 100.0)]),
        ],
        vec![range(vec![], vec![])],
    ))
    .unwrap();
    let a = anim(&r.project).unwrap();
    assert_eq!(
        a.prop("properties/position").unwrap().value,
        Value::Vec3([100.0, 50.0, 0.0])
    );
    assert_eq!(
        a.prop("properties/scale").unwrap().value,
        Value::Vec3([200.0, 180.0, 100.0])
    );
    assert_eq!(
        a.prop("properties/fillColor").unwrap().value,
        Value::Color([10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0, 1.0])
    );
    assert_eq!(
        a.prop("properties/opacity").unwrap().keys[1].time,
        Tick::from_seconds_f64(1.0)
    );
    assert_eq!(
        a.prop("selectors/rangeSelector/end").unwrap().value,
        Value::Scalar(100.0)
    );
    Project::from_json(&r.project.to_file_json().unwrap()).unwrap();
}
#[test]
fn selectors_keep_order_and_advanced_index_parameters() {
    let r = aep_effectcraft::import(&fixture(
        vec![field("ADBE Text Opacity", &[0.0])],
        vec![
            range(vec![field("ADBE Text Percent End", &[50.0])], vec![]),
            range(
                vec![
                    field("ADBE Text Index Start", &[2.0]),
                    field("ADBE Text Index End", &[5.0]),
                ],
                vec![
                    field("ADBE Text Range Units", &[2.0]),
                    field("ADBE Text Selector Mode", &[2.0]),
                    field("ADBE Text Range Type2", &[3.0]),
                    field("ADBE Text Range Shape", &[4.0]),
                    field("ADBE Text Selector Max Amount", &[50.0]),
                ],
            ),
        ],
    ))
    .unwrap();
    let sels = &anim(&r.project)
        .unwrap()
        .group("selectors")
        .unwrap()
        .children;
    assert_eq!(sels.len(), 2);
    let Node::Group(second) = &sels[1] else {
        panic!("selector");
    };
    for (path, expected) in [
        ("advanced/units", 1),
        ("advanced/mode", 1),
        ("advanced/basedOn", 2),
        ("advanced/shape", 3),
    ] {
        assert_eq!(second.prop(path).unwrap().value, Value::Enum(expected));
    }
    assert_eq!(second.prop("end").unwrap().value, Value::Scalar(5.0));
}
#[test]
fn bad_animator_is_a_reported_local_omission() {
    for props in [
        vec![field("ADBE Text Position 3D", &[5.0, 6.0])],
        vec![field("ADBE Text Position 3D", &[5.0, 6.0, 7.0])],
        vec![field("ADBE Text Opacity", &[f64::NAN])],
        vec![
            field("ADBE Text Opacity", &[0.0]),
            field("ADBE Text Opacity", &[100.0]),
        ],
        vec![support::field_with(
            "ADBE Text Opacity",
            &[0.0],
            vec![chunk(b"Utf8", b"throw 'never run'")],
        )],
    ] {
        let r = aep_effectcraft::import(&fixture(props, vec![range(vec![], vec![])])).unwrap();
        assert!(anim(&r.project).is_none());
        assert!(
            r.report
                .warnings
                .iter()
                .any(|w| w.message.contains("Text animator") && w.message.contains("omitted"))
        );
        assert_eq!(r.report.keyframes, 0);
        assert!(
            r.project.comps().next().unwrap().1.layers[0]
                .props
                .prop("text/sourceText")
                .is_some()
        );
    }
}
#[test]
fn unknown_executable_and_excessive_selectors_are_not_default_ranges() {
    for selectors in [
        vec![group("ADBE Text Expressible Selector", vec![], true)],
        vec![group("ADBE Text Wiggly Selector", vec![], true)],
        (0..33).map(|_| range(vec![], vec![])).collect(),
        vec![range(vec![], vec![field("ADBE Text Range Units", &[2.0])])],
        vec![range(
            vec![],
            vec![field("ADBE Text Selector Mode", &[9.0])],
        )],
    ] {
        let r = aep_effectcraft::import(&fixture(
            vec![field("ADBE Text Opacity", &[0.0])],
            selectors,
        ))
        .unwrap();
        assert!(anim(&r.project).is_none());
    }
}
#[test]
#[ignore = "local authored text animator originals"]
fn native_numeric_and_vector_text_animations_survive() {
    let r = aep_effectcraft::import(&native("re_text_animator_animated.aep")).unwrap();
    let a = anim(&r.project).unwrap();
    let offset = a.prop("selectors/rangeSelector/offset").unwrap();
    assert_eq!(offset.keys.len(), 2);
    assert_eq!(offset.keys[1].time, Tick::from_seconds_f64(2.0));
    assert_eq!(
        a.prop("properties/opacity").unwrap().value,
        Value::Scalar(0.0)
    );
    let r = aep_effectcraft::import(&native("re_text_animator_animatedvec.aep")).unwrap();
    let a = anim(&r.project).unwrap();
    for (path, value) in [
        ("position", Value::Vec3([100.0, 50.0, 0.0])),
        ("scale", Value::Vec3([200.0, 200.0, 100.0])),
        ("fillColor", Value::Color([0.0, 0.0, 1.0, 1.0])),
    ] {
        let p = a.prop(&format!("properties/{path}")).unwrap();
        assert_eq!(p.keys[1].value, value);
        assert_eq!(p.keys[1].time, Tick::from_seconds_f64(2.0));
    }
}
#[test]
#[ignore = "local original multi-range and advanced selectors"]
fn native_advanced_and_multiple_ranges_survive() {
    let r = aep_effectcraft::import(&native("re_text_multi_selector.aep")).unwrap();
    assert_eq!(
        anim(&r.project)
            .unwrap()
            .group("selectors")
            .unwrap()
            .children
            .len(),
        2
    );
    let r = aep_effectcraft::import(&native("re_text_range_advanced.aep")).unwrap();
    let a = anim(&r.project).unwrap();
    for (path, value) in [
        ("units", Value::Enum(1)),
        ("basedOn", Value::Enum(1)),
        ("mode", Value::Enum(1)),
        ("amount", Value::Scalar(50.0)),
        ("randomize", Value::Bool(true)),
        ("randomSeed", Value::Scalar(3.0)),
    ] {
        assert_eq!(
            a.prop(&format!("selectors/rangeSelector/advanced/{path}"))
                .unwrap()
                .value,
            value
        );
    }
    assert_eq!(
        a.prop("selectors/rangeSelector/end").unwrap().value,
        Value::Scalar(6.0)
    );
    let r = aep_effectcraft::import(&native("re_text_special_selectors.aep")).unwrap();
    assert!(anim(&r.project).is_none());
}

#[cfg(feature = "render-check")]
#[test]
fn opacity_animator_changes_rendered_text_coverage() {
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
    let mut r = aep_effectcraft::import(&fixture(
        vec![support::keyed_field(
            "ADBE Text Opacity",
            &[(0, 0.0), (30720, 100.0)],
        )],
        vec![range(vec![], vec![])],
    ))
    .unwrap();
    let id = *r.project.comps().next().unwrap().0;
    let layer = &mut r.project.comp_mut(id).unwrap().layers[0];
    layer.switches.video = true;
    layer.in_point = Tick::ZERO;
    for (name, value) in [
        ("position", Value::Vec3([30.0, 120.0, 0.0])),
        ("anchor", Value::Vec3([0.0; 3])),
        ("scale", Value::Vec3([100.0; 3])),
        ("opacity", Value::Scalar(100.0)),
    ] {
        layer
            .props
            .prop_mut(&format!("transform/{name}"))
            .unwrap()
            .value = value;
    }
    let renderer = effectcraft_render::Renderer::new(
        &r.project,
        &NoMedia,
        effectcraft_render::RenderOpts {
            motion_blur: false,
            ..Default::default()
        },
    );
    assert!(
        renderer
            .comp_frame(id, Tick::ZERO)
            .data
            .iter()
            .all(|p| p[3] == 0.0)
    );
    assert!(
        renderer
            .comp_frame(id, Tick::from_seconds_f64(1.0))
            .data
            .iter()
            .any(|p| p[3] > 0.5)
    );
}
