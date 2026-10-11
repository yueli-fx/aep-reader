#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]
mod support;
use effectcraft_keyframe::Value;
use effectcraft_project::{LayerSource, LightKind};
use effectcraft_time::Tick;
use support::{chunk, field, group, list, project_properties, u32_at};

fn leader(separated: bool) -> Vec<u8> {
    let mut bytes = field("ADBE Position", &[10.0, 20.0, 30.0]);
    let at = bytes.windows(4).position(|w| w == b"tdsb").unwrap() + 8;
    bytes[at + 3] = if separated { 3 } else { 1 };
    bytes
}
#[test]
fn separated_axes_keep_distinct_times_and_static_other_axis() {
    let input = project_properties(vec![group(
        "ADBE Transform Group",
        vec![
            leader(true),
            field("ADBE Position_0", &[73.0]),
            support::keyed_field("ADBE Position_1", &[(15360, 40.0), (46080, 80.0)]),
        ],
        true,
    )]);
    let result = aep_effectcraft::import(&input).unwrap();
    let layer = &result.project.comps().next().unwrap().1.layers[0];
    assert_eq!(
        layer.props.prop("transform/positionX").unwrap().value,
        Value::Scalar(73.0)
    );
    let y = layer.props.prop("transform/positionY").unwrap();
    assert_eq!(y.keys.len(), 2);
    assert_eq!(y.keys[0].time, Tick::from_seconds_f64(0.5));
    assert_eq!(y.keys[1].time, Tick::from_seconds_f64(1.5));
    assert_eq!(
        effectcraft_keyframe::evaluate(&y.keys, Tick::from_seconds_f64(1.0), false),
        Some(Value::Scalar(60.0))
    );
    assert_eq!(
        layer.props.prop("transform/positionZ").unwrap().value,
        Value::Scalar(30.0)
    );
    let r = aep_effectcraft::import(&project_properties(vec![group(
        "ADBE Transform Group",
        vec![leader(false), field("ADBE Position_0", &[0.0])],
        true,
    )]))
    .unwrap();
    assert!(
        r.project.comps().next().unwrap().1.layers[0]
            .props
            .prop("transform/positionX")
            .is_none()
    );
}
fn native(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::PathBuf::from(std::env::var_os("AEP_FIXTURE_ROOT").unwrap()).join(name),
    )
    .unwrap()
}
#[test]
#[ignore = "local separation native fixture"]
fn native_separated_position_and_merged_position_are_distinguished() {
    for (name, separated) in [
        ("re_sepdim_merge_before.aep", true),
        ("re_sepdim_merge_after.aep", false),
    ] {
        let result = aep_effectcraft::import(&native(name)).unwrap();
        let layer = &result.project.comps().next().unwrap().1.layers[0];
        assert_eq!(layer.props.prop("transform/positionX").is_some(), separated);
        if separated {
            for (n, v) in [
                ("positionX", 100.0),
                ("positionY", 200.0),
                ("positionZ", 50.0),
            ] {
                assert_eq!(
                    layer.props.prop(&format!("transform/{n}")).unwrap().value,
                    Value::Scalar(v)
                );
            }
        }
    }
}
#[test]
#[ignore = "local camera/light native fixture"]
fn native_camera_and_lights_are_real_layers_with_saved_parameters() {
    let result = aep_effectcraft::import(&native("re_cameralight.aep")).unwrap();
    let camera = result
        .project
        .comps()
        .flat_map(|(_, c)| &c.layers)
        .find(|l| l.name == "MyCamera")
        .unwrap();
    assert_eq!(camera.source, LayerSource::Camera);
    for (n, v) in [
        ("zoom", 1000.0),
        ("focusDistance", 1500.0),
        ("aperture", 50.0),
        ("blurLevel", 75.0),
    ] {
        assert_eq!(
            camera
                .props
                .prop(&format!("cameraOptions/{n}"))
                .unwrap()
                .value,
            Value::Scalar(v)
        );
    }
    assert_eq!(
        camera.props.prop("cameraOptions/dof").unwrap().value,
        Value::Bool(true)
    );
    let light = result
        .project
        .comps()
        .flat_map(|(_, c)| &c.layers)
        .find(|l| l.name == "MySpot")
        .unwrap();
    assert_eq!(
        light.source,
        LayerSource::Light {
            kind: LightKind::Spot
        }
    );
    assert_eq!(
        light.props.prop("lightOptions/intensity").unwrap().value,
        Value::Scalar(150.0)
    );
    assert_eq!(
        light.props.prop("lightOptions/castsShadows").unwrap().value,
        Value::Bool(true)
    );
    effectcraft_project::Project::from_json(&result.project.to_file_json().unwrap()).unwrap();
}
#[test]
#[ignore = "local saved markers"]
fn native_marker_text_and_times_survive_without_guessing_duration() {
    let r = aep_effectcraft::import(&native("re_compmarker.aep")).unwrap();
    let m = r
        .project
        .comps()
        .flat_map(|(_, c)| &c.markers)
        .find(|m| m.comment == "comp marker A")
        .unwrap();
    assert_eq!(m.time, Tick::from_seconds_f64(1.0));
    assert_eq!(m.duration, Tick::ZERO);
    assert!(
        r.report
            .warnings
            .iter()
            .any(|w| w.message.contains("unverified source layout"))
    );
}

fn precomp_remap(animated: bool) -> Vec<u8> {
    let mut input = project_properties(vec![if animated {
        support::keyed_field("ADBE Time Remapping", &[(0, 2.0), (30720, 0.0)])
    } else {
        field("ADBE Time Remapping", &[0.0])
    }]);
    let at = aep_reader::parse(&input)
        .unwrap()
        .root
        .find(b"ldta")
        .unwrap()
        .offset
        + 8;
    u32_at(&mut input, at + 0x28, 8);
    let doc = aep_reader::parse(&input).unwrap();
    let item = doc.root.find_list(b"Item").unwrap();
    let original = list(b"Item", vec![item.data.to_vec()]);
    let mut id = item.child(b"idta").unwrap().data.to_vec();
    u32_at(&mut id, 16, 8);
    let nested = list(
        b"Item",
        vec![
            chunk(b"idta", &id),
            chunk(b"Utf8", b"Nested"),
            chunk(b"cdta", item.child(b"cdta").unwrap().data),
        ],
    );
    let mut body = b"Egg!".to_vec();
    body.extend(list(b"Fold", vec![original, nested]));
    chunk(b"RIFX", &body)
}
#[test]
fn remap_is_enabled_by_saved_keys_and_preserves_source_seconds() {
    for animated in [false, true] {
        let result = aep_effectcraft::import(&precomp_remap(animated)).unwrap();
        let layer = result
            .project
            .comps()
            .flat_map(|(_, c)| &c.layers)
            .next()
            .unwrap();
        assert!(matches!(layer.source, LayerSource::Comp { .. }));
        let p = layer.props.prop("timeRemap");
        assert_eq!(p.is_some(), animated);
        if let Some(p) = p {
            assert_eq!(p.keys.len(), 2);
            assert_eq!(
                effectcraft_keyframe::evaluate(&p.keys, Tick::from_seconds_f64(0.25), false),
                Some(Value::Scalar(1.5))
            );
            assert_eq!(p.keys[1].time, Tick::from_seconds_f64(1.0));
        }
    }
}

#[test]
#[ignore = "local original material fixture"]
fn native_material_switches_keep_zero_based_shadow_modes() {
    let r = aep_effectcraft::import(&native("re_material_classic_2020.aep")).unwrap();
    let l = r
        .project
        .comps()
        .flat_map(|(_, c)| &c.layers)
        .find(|l| l.props.prop("materialOptions/castsShadows").is_some())
        .unwrap();
    assert_eq!(
        l.props.prop("materialOptions/castsShadows").unwrap().value,
        Value::Enum(1)
    );
    assert_eq!(
        l.props
            .prop("materialOptions/acceptsShadows")
            .unwrap()
            .value,
        Value::Enum(0)
    );
    assert_eq!(
        l.props.prop("materialOptions/acceptsLights").unwrap().value,
        Value::Bool(false)
    );
}

#[test]
fn saved_project_depth_and_sample_rate_are_not_replaced_by_defaults() {
    for (code, depth) in [
        (0, effectcraft_project::BitDepth::Bpc8),
        (1, effectcraft_project::BitDepth::Bpc16),
        (2, effectcraft_project::BitDepth::Bpc32),
    ] {
        for rate in [44100.0_f64, 96000.0, f64::NAN, -1.0, 192001.0, 44100.5] {
            let mut header = vec![0; 40];
            header[24] = code;
            let mut body = b"Egg!".to_vec();
            body.extend(chunk(b"nnhd", &header));
            body.extend(chunk(b"adfr", &rate.to_be_bytes()));
            body.extend(chunk(b"lnrb", &[1]));
            body.extend(&project_properties(vec![])[12..]);
            let r = aep_effectcraft::import(&chunk(b"RIFX", &body)).unwrap();
            assert_eq!(r.project.settings.bit_depth, depth);
            assert_eq!(
                r.project.settings.audio_sample_rate,
                if rate == 44100.0 || rate == 96000.0 {
                    rate as u32
                } else {
                    48000
                }
            );
            assert!(
                r.report
                    .warnings
                    .iter()
                    .any(|w| w.message.contains("color-pipeline"))
            );
        }
    }
}
