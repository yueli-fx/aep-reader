#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
mod support;
use effectcraft_keyframe::Value;
use effectcraft_project::{Node, Project, PropGroup};
use effectcraft_time::Tick;
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
fn native(root: &str, name: &str) -> Vec<u8> {
    std::fs::read(std::path::PathBuf::from(std::env::var_os(root).unwrap()).join(name)).unwrap()
}

#[test]
#[ignore = "local native gradient fixtures"]
fn native_gradient_stops_midpoints_and_animation_survive() {
    let static_result =
        aep_effectcraft::import(&native("AEP_FIXTURE_ROOT", "v2_2_gradient_src.aep")).unwrap();
    let fill = first(&static_result.project, "gfill").unwrap();
    let Value::Gradient(g) = &fill.prop("colors").unwrap().value else {
        panic!("gradient")
    };
    assert_eq!(g.colors.len(), 3);
    assert_eq!(g.opacities.len(), 3);
    assert!((g.colors[0].0 - 0.065).abs() < 1e-6);
    assert!((g.sample(g.opacities[1].0)[3] - 0.86).abs() < 1e-5);
    let stroke = first(&static_result.project, "gstroke").unwrap();
    assert_eq!(stroke.prop("width").unwrap().value, Value::Scalar(18.0));
    assert_eq!(stroke.prop("cap").unwrap().value, Value::Enum(1));
    let result =
        aep_effectcraft::import(&native("AEP_FIXTURE_ROOT", "v2_2_gradient_anim_src.aep")).unwrap();
    let fill = first(&result.project, "gfill").unwrap();
    let p = fill.prop("colors").unwrap();
    assert_eq!(p.keys.len(), 2);
    assert_eq!(p.keys[0].time, Tick::ZERO);
    assert_eq!(p.keys[1].time, Tick::from_seconds_f64(1.0));
    let Value::Gradient(mid) =
        effectcraft_keyframe::evaluate(&p.keys, Tick::from_seconds_f64(0.5), false).unwrap()
    else {
        panic!("gradient")
    };
    assert_eq!(mid.sample(0.0), [0.5, 0.5, 0.0, 1.0]);
    assert!(
        result
            .report
            .warnings
            .iter()
            .any(|w| w.message.contains("midpoints"))
    );
    Project::from_json(&result.project.to_file_json().unwrap()).unwrap();
}

fn repeater(copies: f64, scale: f64) -> Vec<u8> {
    group(
        "ADBE Vector Filter - Repeater",
        vec![
            field("ADBE Vector Repeater Copies", &[copies]),
            group(
                "ADBE Vector Repeater Transform",
                vec![
                    field("ADBE Vector Repeater Position", &[60.0, 0.0]),
                    field("ADBE Vector Repeater Scale", &[scale, scale]),
                ],
                true,
            ),
        ],
        true,
    )
}
fn circle() -> Vec<u8> {
    group(
        "ADBE Vector Shape - Ellipse",
        vec![field("ADBE Vector Ellipse Size", &[20.0, 20.0])],
        true,
    )
}
fn fill() -> Vec<u8> {
    group(
        "ADBE Vector Graphic - Fill",
        vec![field("ADBE Vector Fill Color", &[255.0, 255.0, 0.0, 0.0])],
        true,
    )
}

#[test]
fn repeater_expansion_is_bounded_and_does_not_remove_other_shapes() {
    for (copies, scale) in [
        (129.0, 100.0),
        (4.5, 100.0),
        (f64::INFINITY, 100.0),
        (4.0, -100.0),
        (128.0, 200.0),
    ] {
        let result =
            aep_effectcraft::import(&project(vec![circle(), fill(), repeater(copies, scale)]))
                .unwrap();
        assert!(first(&result.project, "repeater").is_none());
        assert!(first(&result.project, "ellipse").is_some());
        assert!(
            result
                .report
                .warnings
                .iter()
                .any(|w| w.message.contains("Repeater omitted"))
        );
    }
    let r = aep_effectcraft::import(&project(vec![
        circle(),
        fill(),
        repeater(128.0, 100.0),
        repeater(128.0, 100.0),
    ]))
    .unwrap();
    assert!(first(&r.project, "repeater").is_none());
}

#[test]
#[ignore = "local native repeater fixture"]
fn native_repeater_keeps_transform_and_opacity() {
    let result =
        aep_effectcraft::import(&native("AEP_SHAPE_FIXTURE_ROOT", "v2_2_repeater.aep")).unwrap();
    let r = first(&result.project, "repeater").unwrap();
    for (name, n) in [
        ("copies", 6.0),
        ("offset", 1.0),
        ("transform/rotation", 15.0),
        ("transform/startOpacity", 95.0),
        ("transform/endOpacity", 40.0),
    ] {
        assert_eq!(r.prop(name).unwrap().value, Value::Scalar(n));
    }
    assert_eq!(
        r.prop("transform/position").unwrap().value,
        Value::Vec2([200.0, 30.0])
    );
    assert_eq!(
        r.prop("transform/scale").unwrap().value,
        Value::Vec2([90.0, 90.0])
    );
    Project::from_json(&result.project.to_file_json().unwrap()).unwrap();
}

#[cfg(feature = "render-check")]
#[test]
fn repeater_renders_three_separate_copies() {
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
    let mut result =
        aep_effectcraft::import(&project(vec![circle(), fill(), repeater(3.0, 100.0)])).unwrap();
    let id = *result.project.comps().next().unwrap().0;
    let layer = &mut result.project.comp_mut(id).unwrap().layers[0];
    layer.switches.video = true;
    layer.in_point = Tick::ZERO;
    for (n, v) in [
        ("position", Value::Vec3([40.0, 128.0, 0.0])),
        ("anchor", Value::Vec3([0.0; 3])),
        ("scale", Value::Vec3([100.0; 3])),
        ("opacity", Value::Scalar(100.0)),
    ] {
        layer
            .props
            .prop_mut(&format!("transform/{n}"))
            .unwrap()
            .value = v;
    }
    let image = effectcraft_render::Renderer::new(
        &result.project,
        &NoMedia,
        effectcraft_render::RenderOpts {
            motion_blur: false,
            ..Default::default()
        },
    )
    .comp_frame(id, Tick::ZERO);
    for x in [40, 100, 160] {
        assert!(image.get(x, 128)[3] > 0.9);
    }
    for x in [70, 130, 220] {
        assert_eq!(image.get(x, 128)[3], 0.0);
    }
}
