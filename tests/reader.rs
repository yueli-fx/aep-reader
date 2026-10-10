#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
use aep_reader::{DecodeStatus, Error, LayerKind, Limits, Value, parse, parse_with_limits};

fn chunk(id: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend(id);
    v.extend((data.len() as u32).to_be_bytes());
    v.extend(data);
    if data.len() % 2 == 1 {
        v.push(0);
    }
    v
}
fn list(form: &[u8; 4], children: Vec<Vec<u8>>) -> Vec<u8> {
    let mut d = form.to_vec();
    for c in children {
        d.extend(c);
    }
    chunk(b"LIST", &d)
}
fn aep(children: Vec<Vec<u8>>) -> Vec<u8> {
    let mut d = b"Egg!".to_vec();
    for c in children {
        d.extend(c);
    }
    chunk(b"RIFX", &d)
}
fn item(kind: u16, id: u32, name: &str, extra: Vec<Vec<u8>>) -> Vec<u8> {
    let mut d = vec![0; 64];
    d[..2].copy_from_slice(&kind.to_be_bytes());
    d[16..20].copy_from_slice(&id.to_be_bytes());
    let mut children = vec![chunk(b"idta", &d), chunk(b"Utf8", name.as_bytes())];
    children.extend(extra);
    list(b"Item", children)
}
fn set32(d: &mut [u8], o: usize, n: u32) {
    d[o..o + 4].copy_from_slice(&n.to_be_bytes());
}
fn cdta() -> Vec<u8> {
    let mut d = vec![0; 204];
    set32(&mut d, 8, 30720);
    d[6..8].copy_from_slice(&1024u16.to_be_bytes());
    d[0x8c..0x8e].copy_from_slice(&1920u16.to_be_bytes());
    d[0x8e..0x90].copy_from_slice(&1080u16.to_be_bytes());
    set32(&mut d, 0x9c, 30 * 65536);
    set32(&mut d, 0x90, 1);
    set32(&mut d, 0x94, 1);
    set32(&mut d, 0xb0, 150);
    set32(&mut d, 0x24, u32::MAX);
    d[0x34..0x37].copy_from_slice(&[255, 128, 0]);
    chunk(b"cdta", &d)
}
fn prop(name: &str, value: f64) -> Vec<Vec<u8>> {
    let mut d = vec![0; 124];
    d[3] = 1;
    vec![
        chunk(b"tdmn", name.as_bytes()),
        list(
            b"tdbs",
            vec![chunk(b"tdb4", &d), chunk(b"cdat", &value.to_be_bytes())],
        ),
    ]
}
fn project(props: Vec<Vec<u8>>) -> Vec<u8> {
    let mut l = vec![0; 164];
    set32(&mut l, 0, 12);
    set32(&mut l, 8, 1);
    set32(&mut l, 0x6c, 1);
    set32(&mut l, 0x10, 30720);
    set32(&mut l, 0x14, 30720);
    set32(&mut l, 0x18, 30720);
    set32(&mut l, 0x1c, 153600);
    set32(&mut l, 0x20, 30720);
    l[0x27] = 7;
    aep(vec![list(
        b"Fold",
        vec![item(
            4,
            1,
            "Main",
            vec![
                cdta(),
                list(
                    b"Layr",
                    vec![
                        chunk(b"ldta", &l),
                        chunk(b"Utf8", b"Layer"),
                        list(b"tdgp", props),
                    ],
                ),
            ],
        )],
    )])
}
fn animated(count: u32, stride: u32, data: &[u8]) -> Vec<Vec<u8>> {
    let mut desc = vec![0; 124];
    desc[3] = 1;
    let mut h = vec![0; 52];
    set32(&mut h, 8, count);
    set32(&mut h, 16, stride);
    vec![
        chunk(b"tdmn", b"ADBE Opacity"),
        list(
            b"tdbs",
            vec![
                chunk(b"tdb4", &desc),
                list(b"list", vec![chunk(b"lhd3", &h), chunk(b"ldat", data)]),
            ],
        ),
    ]
}

#[test]
fn semantic_comp_layer_and_static_property() {
    let bytes = project(prop("ADBE Opacity", 42.5));
    let d = parse(&bytes).unwrap();
    let c = &d.project.compositions[0];
    assert_eq!(
        (c.width, c.height, c.frame_rate, c.duration_seconds),
        (Some(1920), Some(1080), Some(30.0), Some(5.0))
    );
    assert!(c.work_area_uses_duration);
    assert_eq!(c.background_rgb, Some([255, 128, 0]));
    let l = &c.layers[0];
    assert_eq!(l.id, 12);
    assert_eq!(l.kind, LayerKind::Null);
    assert_eq!(l.source_in_point.unwrap().value(), Some(1.0));
    assert_eq!(l.source_out_point.unwrap().value(), Some(5.0));
    assert_eq!(l.stretch.unwrap().value(), Some(1.0));
    assert_eq!(
        l.property("ADBE Opacity")
            .unwrap()
            .static_value
            .as_ref()
            .unwrap()
            .numbers()
            .unwrap()[0]
            .value,
        Some(42.5)
    );
}
#[test]
fn keyframe_time_values_and_ratio_easing() {
    let mut k = vec![0; 48];
    set32(&mut k, 0, 15360);
    k[4] = 2;
    k[5] = 3;
    k[8..16].copy_from_slice(&80.0f64.to_be_bytes());
    k[16..24].copy_from_slice(&2.0f64.to_be_bytes());
    k[24..32].copy_from_slice(&0.25f64.to_be_bytes());
    k[32..40].copy_from_slice(&3.0f64.to_be_bytes());
    k[40..48].copy_from_slice(&0.5f64.to_be_bytes());
    let bytes = project(animated(1, 48, &k));
    let d = parse(&bytes).unwrap();
    let k = &d.project.compositions[0].layers[0]
        .property("ADBE Opacity")
        .unwrap()
        .keyframes[0];
    assert_eq!(k.time_seconds, 0.5);
    assert_eq!(k.in_ease[0].influence.value, Some(0.25));
    assert_eq!(k.out_ease[0].speed.value, Some(3.0));
    assert_eq!(k.out_interpolation, 3);
    assert_eq!(
        k.value.as_ref().unwrap().numbers().unwrap()[0].value,
        Some(80.0)
    );
}

#[test]
fn spatial_high_flags_keep_values_and_tangents_at_spatial_offsets() {
    let mut descriptor = vec![0; 124];
    descriptor[3] = 3;
    let mut header = vec![0; 52];
    set32(&mut header, 8, 3);
    set32(&mut header, 16, 128);
    let mut records = Vec::new();
    for (i, flags) in [7, 15, 31].into_iter().enumerate() {
        let mut record = vec![0; 128];
        set32(&mut record, 0, i as u32 * 30720);
        record[7] = flags;
        for (j, value) in [10.0_f64, 20.0, 30.0, -1.0, -2.0, -3.0, 4.0, 5.0, 6.0]
            .into_iter()
            .enumerate()
        {
            let at = 0x38 + j * 8;
            record[at..at + 8].copy_from_slice(&value.to_be_bytes());
        }
        records.extend(record);
    }
    let make = |data: &[u8]| {
        project(vec![
            chunk(b"tdmn", b"ADBE Position"),
            list(
                b"tdbs",
                vec![
                    chunk(b"tdb4", &descriptor),
                    list(b"list", vec![chunk(b"lhd3", &header), chunk(b"ldat", data)]),
                ],
            ),
        ])
    };
    let bytes = make(&records);
    let doc = parse(&bytes).unwrap();
    let property = doc.project.compositions[0].layers[0]
        .property("ADBE Position")
        .unwrap();
    assert_eq!(property.keyframes.len(), 3);
    for (key, flag) in property.keyframes.iter().zip([7, 15, 31]) {
        assert_eq!(key.raw_flags[1], flag);
        assert_eq!(
            key.value
                .as_ref()
                .unwrap()
                .numbers()
                .unwrap()
                .iter()
                .map(|v| v.value.unwrap())
                .collect::<Vec<_>>(),
            [10.0, 20.0, 30.0]
        );
        assert_eq!(
            key.in_spatial_tangent
                .iter()
                .map(|v| v.value.unwrap())
                .collect::<Vec<_>>(),
            [-1.0, -2.0, -3.0]
        );
        assert_eq!(
            key.out_spatial_tangent
                .iter()
                .map(|v| v.value.unwrap())
                .collect::<Vec<_>>(),
            [4.0, 5.0, 6.0]
        );
    }
    let bytes = make(&records[..records.len() - 1]);
    let malformed = parse(&bytes).unwrap();
    assert!(
        malformed
            .project
            .diagnostics
            .iter()
            .any(|d| d.code == "invalid-keyframes")
    );
}
#[test]
fn nested_folders_preserve_panel_parent_and_order() {
    let bytes = aep(vec![list(
        b"Fold",
        vec![
            item(
                1,
                5,
                "Folder",
                vec![list(b"Sfdr", vec![item(4, 7, "Nested", vec![cdta()])])],
            ),
            item(7, 8, "Asset", vec![]),
        ],
    )]);
    let d = parse(&bytes).unwrap();
    assert_eq!(
        d.project
            .items
            .iter()
            .map(|i| (i.id, i.parent_id, i.order))
            .collect::<Vec<_>>(),
        [(5, 0, 0), (7, 5, 0), (8, 0, 1)]
    );
}
#[test]
fn repeated_and_opaque_properties_are_not_dropped() {
    let mut p = prop("ADBE Opacity", 1.0);
    p.extend(prop("ADBE Opacity", 2.0));
    p.extend([
        chunk(b"tdmn", b"Unknown future property"),
        chunk(b"xxxx", b"opaque"),
    ]);
    let bytes = project(p);
    let d = parse(&bytes).unwrap();
    let p = &d.project.compositions[0].layers[0].properties;
    assert_eq!(p.len(), 3);
    assert_eq!(p[2].decode_status, DecodeStatus::Unsupported);
    assert!(!d.project.diagnostics.is_empty());
}
#[test]
fn malformed_and_truncated_headers_are_errors() {
    for bytes in [
        b"".as_slice(),
        b"RIFX",
        b"RIFX\0\0\0\x04Egg",
        b"RIFF\0\0\0\x04Egg!",
        b"RIFX\0\0\0\x04xxxx",
    ] {
        assert!(parse(bytes).is_err());
    }
    let mut bytes = aep(vec![]);
    bytes[4..8].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(parse(&bytes).is_err());
    let bytes = aep(vec![chunk(b"Utf8", b"a")]);
    for len in 0..bytes.len() {
        assert!(parse(&bytes[..len]).is_err());
    }
}
#[test]
fn rejects_missing_padding_and_cross_parent_chunks() {
    let mut bytes = aep(vec![chunk(b"Utf8", b"a")]);
    bytes.pop();
    let n = (bytes.len() - 8) as u32;
    bytes[4..8].copy_from_slice(&n.to_be_bytes());
    assert!(parse(&bytes).is_err());
    let mut bytes = aep(vec![list(b"Fold", vec![chunk(b"Utf8", b"abcd")])]);
    bytes[28..32].copy_from_slice(&100u32.to_be_bytes());
    assert!(parse(&bytes).is_err());
}
#[test]
fn resource_limits_fail_before_large_allocations() {
    let bytes = project(prop("ADBE Opacity", 1.0));
    for limits in [
        Limits {
            max_input_bytes: 4,
            ..Limits::default()
        },
        Limits {
            max_chunks: 2,
            ..Limits::default()
        },
        Limits {
            max_chunk_bytes: 4,
            ..Limits::default()
        },
        Limits {
            max_allocated_bytes: 1,
            ..Limits::default()
        },
        Limits {
            max_depth: 1,
            ..Limits::default()
        },
    ] {
        assert!(matches!(
            parse_with_limits(&bytes, &limits),
            Err(Error::ResourceLimit { .. })
        ));
    }
    let limits = Limits {
        max_depth: 0,
        ..Limits::default()
    };
    assert!(matches!(
        parse_with_limits(&bytes, &limits),
        Err(Error::InvalidLimits(_))
    ));
}
#[test]
fn invalid_keyframe_counts_are_diagnostic_without_panics() {
    let bytes = project(animated(u32::MAX, 48, b""));
    let d = parse(&bytes).unwrap();
    let l = &d.project.compositions[0].layers[0];
    assert!(l.property("ADBE Opacity").unwrap().keyframes.is_empty());
    assert_eq!(
        l.node("ADBE Opacity").unwrap().decode_status,
        DecodeStatus::Partial
    );
    assert!(
        d.project
            .diagnostics
            .iter()
            .any(|d| d.code == "invalid-keyframes")
    );
}
#[test]
fn cumulative_keyframe_limit_and_zero_count() {
    let bytes = project(animated(0, 48, b""));
    assert!(parse(&bytes).is_ok());
    let bytes = project(animated(2, 48, &[0; 96]));
    let limits = Limits {
        max_keyframes: 1,
        ..Limits::default()
    };
    assert!(matches!(
        parse_with_limits(&bytes, &limits),
        Err(Error::ResourceLimit {
            resource: "keyframe count",
            ..
        })
    ));
}
#[test]
fn opaque_text_and_trailing_bytes_stay_accessible() {
    let mut bytes = aep(vec![list(b"btds", vec![b"not chunks".to_vec()])]);
    bytes.extend(b"tail");
    let d = parse(&bytes).unwrap();
    assert_eq!(d.trailing, b"tail");
    let c = d.root.list(b"btds").unwrap();
    assert!(c.opaque);
    assert_eq!(c.data, b"not chunks");
    assert_eq!(c.data.as_ptr(), bytes[24..].as_ptr());
}
#[test]
fn non_finite_values_keep_their_categories() {
    let bytes = project(prop("ADBE Opacity", f64::NAN));
    let d = parse(&bytes).unwrap();
    let p = d.project.compositions[0].layers[0]
        .property("ADBE Opacity")
        .unwrap();
    let Some(Value::Numbers(v)) = &p.static_value else {
        panic!("numeric value");
    };
    assert_eq!(v[0].value, None);
    assert_eq!(v[0].non_finite, Some(aep_reader::NonFinite::NaN));
    #[cfg(feature = "serde")]
    {
        assert!(serde_json::to_string(&d.project).unwrap().contains("NaN"));
    }
}
#[test]
fn corruption_smoke_does_not_panic() {
    let seed = project(prop("ADBE Opacity", 12.0));
    for i in 0..seed.len() {
        let mut bytes = seed.clone();
        bytes[i] ^= 0xff;
        let _ = parse(&bytes);
    }
}

#[test]
fn reads_static_mask_path_and_source_span() {
    let mut h = vec![0; 52];
    set32(&mut h, 8, 3);
    set32(&mut h, 16, 8);
    let mut data = Vec::new();
    for xy in [[10.0f32, 20.0], [8.0, 19.0], [12.0, 21.0]] {
        for n in xy {
            data.extend(n.to_be_bytes());
        }
    }
    let mut shph = vec![0; 24];
    shph[3] = 9;
    let shap = list(
        b"shap",
        vec![
            chunk(b"shph", &shph),
            list(b"list", vec![chunk(b"lhd3", &h), chunk(b"ldat", &data)]),
        ],
    );
    let bytes = project(vec![
        chunk(b"tdmn", b"ADBE Mask Shape"),
        list(b"om-s", vec![list(b"omks", vec![shap])]),
    ]);
    let doc = parse(&bytes).unwrap();
    let node = doc.project.compositions[0].layers[0]
        .node("ADBE Mask Shape")
        .unwrap();
    let Some(Value::Path(path)) = &node.property.as_ref().unwrap().static_value else {
        panic!("path");
    };
    assert_eq!(path.closed, Some(false));
    assert_eq!(path.vertices[0].anchor[1].value, Some(20.0));
    assert_eq!(path.vertices[0].in_tangent[0].value, Some(12.0));
    assert_eq!(path.vertices[0].out_tangent[0].value, Some(8.0));
    assert_eq!(
        doc.bytes(node.source).unwrap().get(..4),
        Some(b"LIST".as_slice())
    );
}

fn text_project(body: &[u8]) -> Vec<u8> {
    project(vec![
        chunk(b"tdmn", b"ADBE Text Document"),
        list(b"btds", vec![list(b"btdk", vec![body.to_vec()])]),
    ])
}
#[test]
fn utf16_text_handles_surrogates_and_parentheses_escapes() {
    let mut body = b"/1 << /1 [ << /0 << /0 (".to_vec();
    body.extend([0xfe, 0xff]);
    for unit in "你好😀\r".encode_utf16() {
        body.extend(unit.to_be_bytes());
    }
    body.extend(b") >> >> ] >>");
    let bytes = text_project(&body);
    let d = parse(&bytes).unwrap();
    let l = &d.project.compositions[0].layers[0];
    assert_eq!(l.kind, LayerKind::Text);
    assert_eq!(l.text_source.as_ref().unwrap().text, "你好😀\r");
    let bytes = text_project(b"/1 << /1 [ << /0 << /0 (one\\(two\\)\\050x\\051) >> >> ] >>");
    let d = parse(&bytes).unwrap();
    assert_eq!(
        d.project.compositions[0].layers[0]
            .text_source
            .as_ref()
            .unwrap()
            .text,
        "one(two)(x)"
    );
}
#[test]
fn malformed_text_is_diagnostic_and_depth_is_bounded() {
    let bytes = text_project(b"/1 << /1 [(");
    let d = parse(&bytes).unwrap();
    assert!(d.project.compositions[0].layers[0].text_source.is_none());
    assert!(
        d.project
            .diagnostics
            .iter()
            .any(|n| n.code == "unsupported-text")
    );
    let mut body = b"/0 ".to_vec();
    body.extend(std::iter::repeat_n(b'[', 70));
    body.extend(b"0");
    body.extend(std::iter::repeat_n(b']', 70));
    let bytes = text_project(&body);
    assert!(matches!(
        parse(&bytes),
        Err(Error::ResourceLimit {
            resource: "text dictionary depth",
            ..
        })
    ));
}

#[test]
fn color_depth_codes_map_to_bits_and_unknown_values_stay_unknown() {
    for (code, expected) in [(0, Some(8)), (1, Some(16)), (2, Some(32)), (99, None)] {
        let mut header = vec![0; 32];
        header[15] = code;
        let bytes = aep(vec![chunk(b"nhed", &header)]);
        let d = parse(&bytes).unwrap();
        assert_eq!(d.project.bits_per_channel, expected);
        assert_eq!(d.project.color_depth_code, Some(code));
        if expected.is_none() {
            assert!(
                d.project
                    .diagnostics
                    .iter()
                    .any(|n| n.code == "unknown-color-depth")
            );
        }
    }
}

#[test]
fn composition_duration_uses_its_saved_denominator() {
    for (numerator, denominator, expected) in [(600, 30, 20.0), (160, 40, 4.0), (0, 30, 0.0)] {
        let mut data = cdta()[8..].to_vec();
        set32(&mut data, 0x2c, numerator);
        set32(&mut data, 0x30, denominator);
        let bytes = aep(vec![list(
            b"Fold",
            vec![item(4, 1, "Duration", vec![chunk(b"cdta", &data)])],
        )]);
        assert_eq!(
            parse(&bytes).unwrap().project.compositions[0].duration_seconds,
            Some(expected)
        );
    }
}

#[test]
fn malformed_duration_denominator_reports_fallback() {
    let bytes = project(Vec::new());
    let parsed = parse(&bytes).unwrap();
    assert_eq!(parsed.project.compositions[0].duration_seconds, Some(5.0));
    assert!(
        parsed
            .project
            .diagnostics
            .iter()
            .any(|d| d.code == "composition-duration-fallback")
    );
    let data = vec![0; 0x30];
    let bytes = aep(vec![list(
        b"Fold",
        vec![item(4, 1, "Short", vec![chunk(b"cdta", &data)])],
    )]);
    assert_eq!(
        parse(&bytes).unwrap().project.compositions[0].duration_seconds,
        None
    );
}

#[test]
fn negative_composition_display_start_is_signed() {
    let mut data = cdta()[8..].to_vec();
    set32(&mut data, 0xa4, u32::MAX);
    set32(&mut data, 0xa8, 24);
    let bytes = aep(vec![list(
        b"Fold",
        vec![item(4, 1, "Negative", vec![chunk(b"cdta", &data)])],
    )]);
    assert_eq!(
        parse(&bytes).unwrap().project.compositions[0]
            .display_start
            .unwrap()
            .value(),
        Some(-1.0 / 24.0)
    );
}
