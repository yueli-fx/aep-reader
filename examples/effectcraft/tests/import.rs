#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
use aep_effectcraft::import;
use effectcraft_keyframe::Value;
use effectcraft_project::Project;
use effectcraft_time::{TICKS_PER_SECOND, Tick};

fn chunk(id: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = id.to_vec();
    out.extend((data.len() as u32).to_be_bytes());
    out.extend(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
    out
}
fn list(form: &[u8; 4], children: Vec<Vec<u8>>) -> Vec<u8> {
    let mut data = form.to_vec();
    data.extend(children.into_iter().flatten());
    chunk(b"LIST", &data)
}
fn u32_at(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_be_bytes());
}
fn fixture(parent: u32, stretch: u32, animated: bool, value: f64) -> Vec<u8> {
    let mut item = vec![0; 64];
    item[..2].copy_from_slice(&4u16.to_be_bytes());
    u32_at(&mut item, 16, 7);
    let mut comp = vec![0; 204];
    comp[6..8].copy_from_slice(&1024u16.to_be_bytes());
    u32_at(&mut comp, 8, 30720);
    comp[0x8c..0x8e].copy_from_slice(&1920u16.to_be_bytes());
    comp[0x8e..0x90].copy_from_slice(&1080u16.to_be_bytes());
    u32_at(&mut comp, 0x9c, 30 * 65536);
    u32_at(&mut comp, 0x90, 1);
    u32_at(&mut comp, 0x94, 1);
    u32_at(&mut comp, 0xb0, 150);
    u32_at(&mut comp, 0xa8, 30720);
    let mut layer = vec![0; 164];
    u32_at(&mut layer, 0, 12);
    u32_at(&mut layer, 8, stretch);
    u32_at(&mut layer, 0x6c, 1);
    u32_at(&mut layer, 0x10, 30720);
    u32_at(&mut layer, 0x14, 30720);
    u32_at(&mut layer, 0x18, 30720);
    u32_at(&mut layer, 0x1c, 153600);
    u32_at(&mut layer, 0x20, 30720);
    u32_at(&mut layer, 0x84, parent);
    layer[0x27] = 7;
    let mut desc = vec![0; 124];
    desc[3] = 1;
    let mut props = vec![
        chunk(b"tdb4", &desc),
        chunk(b"cdat", &value.to_be_bytes()),
        chunk(b"Utf8", b"throw new Error('must not execute')"),
    ];
    if animated {
        let mut header = vec![0; 52];
        u32_at(&mut header, 8, 1);
        u32_at(&mut header, 16, 48);
        let mut key = vec![0; 48];
        u32_at(&mut key, 0, 15360);
        key[8..16].copy_from_slice(&0.8f64.to_be_bytes());
        props.push(list(
            b"list",
            vec![chunk(b"lhd3", &header), chunk(b"ldat", &key)],
        ));
    }
    let item = list(
        b"Item",
        vec![
            chunk(b"idta", &item),
            chunk(b"Utf8", b"Main"),
            chunk(b"cdta", &comp),
            list(
                b"Layr",
                vec![
                    chunk(b"ldta", &layer),
                    chunk(b"Utf8", b"Null"),
                    list(
                        b"tdgp",
                        vec![chunk(b"tdmn", b"ADBE Opacity"), list(b"tdbs", props)],
                    ),
                ],
            ),
        ],
    );
    let mut root = b"Egg!".to_vec();
    root.extend(list(b"Fold", vec![item]));
    chunk(b"RIFX", &root)
}

#[test]
fn actual_effectcraft_reopens_mapped_project() {
    let result = import(&fixture(0, 1, false, 0.425)).unwrap();
    let json = result.project.to_file_json().unwrap();
    let reopened = Project::from_json(&json).unwrap();
    let (_, comp) = reopened.comps().next().unwrap();
    assert_eq!((comp.width, comp.height), (1920, 1080));
    assert_eq!(comp.frame_rate.as_f64(), 30.0);
    assert_eq!(comp.duration, Tick(5 * TICKS_PER_SECOND));
    let layer = &comp.layers[0];
    assert!(layer.switches.effects); // Empty effect stack does not turn off the layer switch.
    assert_eq!(layer.in_point, Tick(TICKS_PER_SECOND));
    assert_eq!(layer.out_point, Tick(5 * TICKS_PER_SECOND));
    assert_eq!(layer.stretch, 100.0);
    let prop = layer.props.prop("transform/opacity").unwrap();
    assert_eq!(prop.value, Value::Scalar(42.5));
    let expr = prop.expr.as_ref().unwrap();
    assert!(!expr.enabled);
    assert!(expr.text.contains("must not execute"));
    assert!(
        result
            .report
            .warnings
            .iter()
            .any(|w| w.message.contains("disabled"))
    );
}

#[test]
fn keyframe_time_is_converted_without_frame_rounding() {
    let result = import(&fixture(0, 1, true, 0.425)).unwrap();
    let (_, comp) = result.project.comps().next().unwrap();
    let key = &comp.layers[0].props.prop("transform/opacity").unwrap().keys[0];
    assert_eq!(key.time, Tick(TICKS_PER_SECOND / 2));
    assert_eq!(key.value, Value::Scalar(80.0));
    assert!(
        result
            .report
            .warnings
            .iter()
            .any(|w| w.message.contains("interpolation is linear"))
    );
}

#[test]
fn bad_container_truncation_and_nonfinite_values_return_errors() {
    assert!(import(b"not an AEP").is_err());
    let bytes = fixture(0, 1, false, 1.0);
    for end in 0..bytes.len() {
        assert!(import(&bytes[..end]).is_err());
    }
    assert!(import(&fixture(0, 1, false, f64::NAN)).is_err());
}

#[test]
fn cyclic_dangling_parents_and_zero_stretch_return_errors() {
    assert!(
        import(&fixture(12, 1, false, 1.0))
            .err()
            .unwrap()
            .contains("cyclic")
    );
    assert!(
        import(&fixture(99, 1, false, 1.0))
            .err()
            .unwrap()
            .contains("parent")
    );
    assert!(
        import(&fixture(0, 0, false, 1.0))
            .err()
            .unwrap()
            .contains("stretch")
    );
}

#[test]
fn reverse_stretch_keeps_ordered_comp_bounds() {
    let result = import(&fixture(0, (-2_i32) as u32, false, 1.0)).unwrap();
    let (_, comp) = result.project.comps().next().unwrap();
    let layer = &comp.layers[0];
    assert_eq!(layer.stretch, -200.0);
    assert_eq!(layer.in_point, Tick(-10 * TICKS_PER_SECOND));
    assert_eq!(layer.out_point, Tick(-2 * TICKS_PER_SECOND));
}

#[test]
fn self_referencing_precomp_returns_error() {
    let mut bytes = fixture(0, 1, false, 42.0);
    let layer = bytes.windows(4).position(|w| w == b"ldta").unwrap() + 8;
    u32_at(&mut bytes, layer + 0x28, 7);
    assert!(import(&bytes).err().unwrap().contains("cyclic precomp"));
}

#[test]
#[ignore = "requires local, non-redistributed AEP fixtures via AEP_FIXTURE_ROOT"]
fn real_files_reopen_and_preserve_mapped_facts() {
    let root = std::path::PathBuf::from(
        std::env::var_os("AEP_FIXTURE_ROOT").expect("set AEP_FIXTURE_ROOT"),
    );
    for name in [
        "transform_unseparated.aep",
        "re_solidnull.aep",
        "re_precomp.aep",
        "orientation_with_keyframes.aep",
        "re_text_multirun.aep",
    ] {
        let bytes = std::fs::read(root.join(name)).unwrap();
        let original = aep_reader::parse(&bytes).unwrap();
        let result = import(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let reopened = Project::from_json(&result.project.to_file_json().unwrap()).unwrap();
        assert_eq!(
            original.project.compositions.len(),
            reopened.comps().count(),
            "{name}"
        );
        for source in &original.project.compositions {
            let target = reopened
                .items
                .values()
                .find(|i| i.name == source.name)
                .unwrap()
                .as_comp()
                .unwrap();
            assert_eq!(Some(target.width as u16), source.width, "{name}");
            assert_eq!(Some(target.height as u16), source.height, "{name}");
            assert_eq!(source.layers.len(), target.layers.len(), "{name}");
            for (a, b) in source.layers.iter().zip(&target.layers) {
                assert_eq!(a.name, b.name, "{name}");
                for (ae, ec) in [
                    ("ADBE Position", "transform/position"),
                    ("ADBE Opacity", "transform/opacity"),
                    ("ADBE Orientation", "transform/orientation"),
                ] {
                    if let Some(p) = a.property(ae) {
                        let out = b.props.prop(ec).unwrap();
                        assert_eq!(p.keyframes.len(), out.keys.len(), "{name}");
                        for (key, mapped) in p.keyframes.iter().zip(&out.keys) {
                            assert!(
                                (mapped.time.seconds() - key.time_seconds).abs() < 1e-9,
                                "{name}"
                            );
                        }
                        if let Some(numbers) = p.static_value.as_ref().and_then(|v| v.numbers()) {
                            let expected: Vec<_> =
                                numbers.iter().map(|v| v.value.unwrap()).collect();
                            match &out.value {
                                Value::Scalar(v) => assert_eq!(
                                    expected
                                        .iter()
                                        .map(|n| if ae == "ADBE Opacity" { n * 100.0 } else { *n })
                                        .collect::<Vec<_>>(),
                                    vec![*v],
                                    "{name}"
                                ),
                                Value::Vec3(v) => assert_eq!(expected, v.to_vec(), "{name}"),
                                _ => panic!("unexpected mapped type"),
                            }
                        }
                    }
                }
            }
        }
        println!(
            "{name}: {} comps, {} layers, {} keys; reopened",
            result.report.compositions, result.report.layers, result.report.keyframes
        );
    }
}

#[test]
fn compositing_byte_codes_and_self_matte_are_checked() {
    let original = fixture(0, 1, false, 1.0);
    let offset = aep_reader::parse(&original)
        .unwrap()
        .root
        .find_all(b"ldta")
        .next()
        .unwrap()
        .offset
        + 8;
    for (code, expected) in [
        (2, effectcraft_color::BlendMode::Normal),
        (5, effectcraft_color::BlendMode::Multiply),
        (6, effectcraft_color::BlendMode::Screen),
        (38, effectcraft_color::BlendMode::Divide),
    ] {
        let mut input = original.clone();
        input[offset + 0x63] = code;
        let out = import(&input).unwrap();
        assert_eq!(
            out.project.comps().next().unwrap().1.layers[0].blend_mode,
            expected
        );
    }
    let mut input = original.clone();
    input[offset + 0x63] = 255;
    assert!(
        import(&input)
            .unwrap()
            .report
            .warnings
            .iter()
            .any(|w| w.message.contains("Unknown blend"))
    );
    input[offset + 0x6b] = 1;
    u32_at(&mut input, offset + 0xa0, 12);
    assert!(import(&input).err().unwrap().contains("matte graph"));
    u32_at(&mut input, offset + 0xa0, 999);
    assert!(import(&input).err().unwrap().contains("dangling matte"));
}

#[test]
fn invalid_ease_retains_key_values_with_a_diagnostic() {
    for (offset, value) in [(16, f64::NAN), (24, 2.0)] {
        let mut bytes = fixture(0, 1, true, 0.425);
        let at = bytes.windows(4).position(|s| s == b"ldat").unwrap() + 8;
        bytes[at + offset..at + offset + 8].copy_from_slice(&value.to_be_bytes());
        let result = import(&bytes).unwrap();
        let comp = result.project.comps().next().unwrap().1;
        let key = &comp.layers[0].props.prop("transform/opacity").unwrap().keys[0];
        assert_eq!(key.value, Value::Scalar(80.0));
        assert_eq!(key.time, Tick(TICKS_PER_SECOND / 2));
        assert!(key.in_ease.is_empty());
        assert!(
            result
                .report
                .warnings
                .iter()
                .any(|w| w.message.contains("Invalid temporal ease"))
        );
    }
}

#[test]
fn undecoded_key_value_is_local_to_its_property() {
    let mut bytes = fixture(0, 1, true, 0.425);
    let at = bytes.windows(4).position(|s| s == b"lhd3").unwrap() + 8;
    u32_at(&mut bytes, at + 16, 8);
    let result = import(&bytes).unwrap();
    let comp = result.project.comps().next().unwrap().1;
    let property = comp.layers[0].props.prop("transform/opacity").unwrap();
    assert_eq!(property.value, Value::Scalar(100.0));
    assert!(property.keys.is_empty());
    assert!(
        result
            .report
            .warnings
            .iter()
            .any(|w| w.message.contains("Undecoded keyframe value"))
    );
}
