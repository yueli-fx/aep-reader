#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
use aep_reader::{
    AutoOrient, DecodeStatus, Error, Limits, PropertyValueType, Value, parse, parse_with_limits,
};

fn chunk(id: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = id.to_vec();
    out.extend((data.len() as u32).to_be_bytes());
    out.extend(data);
    if data.len() % 2 != 0 {
        out.push(0);
    }
    out
}
fn container(id: &[u8; 4], form: &[u8; 4], children: Vec<Vec<u8>>) -> Vec<u8> {
    let mut out = form.to_vec();
    for c in children {
        out.extend(c);
    }
    chunk(id, &out)
}
fn list(form: &[u8; 4], children: Vec<Vec<u8>>) -> Vec<u8> {
    container(b"LIST", form, children)
}
fn aep(children: Vec<Vec<u8>>) -> Vec<u8> {
    container(b"RIFX", b"Egg!", children)
}
fn set32(d: &mut [u8], offset: usize, n: u32) {
    d[offset..offset + 4].copy_from_slice(&n.to_be_bytes());
}
fn item(kind: u16, extra: Vec<Vec<u8>>) -> Vec<u8> {
    let mut id = vec![0; 64];
    id[..2].copy_from_slice(&kind.to_be_bytes());
    set32(&mut id, 16, 1);
    let mut out = vec![chunk(b"idta", &id), chunk(b"Utf8", b"Main")];
    out.extend(extra);
    list(b"Item", out)
}
fn comp(extra: Vec<Vec<u8>>) -> Vec<u8> {
    let mut d = vec![0; 204];
    set32(&mut d, 8, 24000);
    set32(&mut d, 0x9c, 24 * 65536);
    set32(&mut d, 0xc4, 128);
    set32(&mut d, 0xc8, 16);
    d[..4].copy_from_slice(&[0, 2, 0, 3]);
    let mut out = vec![chunk(b"cdta", &d)];
    out.extend(extra);
    item(4, out)
}
fn layer(props: Vec<Vec<u8>>, flags: [u8; 3]) -> Vec<u8> {
    let mut d = vec![0; 164];
    set32(&mut d, 0, 7);
    d[0x25..0x28].copy_from_slice(&flags);
    d[0x67] = 1;
    list(b"Layr", vec![chunk(b"ldta", &d), list(b"tdgp", props)])
}
fn project(props: Vec<Vec<u8>>) -> Vec<u8> {
    aep(vec![list(
        b"Fold",
        vec![comp(vec![layer(props, [0, 0, 1])])],
    )])
}
fn property(name: &str, desc: &[u8], more: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    let mut children = vec![chunk(b"tdb4", desc)];
    children.extend(more);
    vec![chunk(b"tdmn", name.as_bytes()), list(b"tdbs", children)]
}
fn scalar(value: &Option<Value>) -> Option<f64> {
    value.as_ref()?.numbers()?.first()?.value
}

#[test]
fn project_settings_and_cms_are_read_from_input() {
    let mut n = vec![0; 40];
    n[8] = 0x81;
    n[9] = 2;
    n[11] = 1;
    n[14..16].copy_from_slice(&30u16.to_be_bytes());
    set32(&mut n, 16, 40);
    n[20] = 3;
    n[25] = 1;
    let mut head = vec![0; 20];
    set32(&mut head, 4, (3 << 26) | (1 << 19) | (2 << 15) | 17);
    head[18..20].copy_from_slice(&9u16.to_be_bytes());
    let bytes = aep(vec![chunk(b"head", &head), chunk(b"nnhd", &n), chunk(b"acer", &[1]), chunk(b"adfr", &48000.0f64.to_be_bytes()), chunk(b"dwga", &[1]), chunk(b"lnrb", &[1]), list(b"ExEn", vec![chunk(b"Utf8", b"javascript-1.0")]), list(b"gpuG", vec![chunk(b"Utf8", b"device-id")]),
        chunk(b"Utf8", br#"{"colorManagementSystem":1,"lutInterpolationMethod":0,"ocioConfigurationFile":"C:\\configs\\\u6708\ud83d\ude00.ocio","baseColorProfile":{"colorProfileName":"sRGB"},"other":[true,null,{"x":3}]}"#), list(b"Pefl", vec![chunk(b"pjef", b"ADBE Fill")])]);
    let d = parse(&bytes).unwrap();
    let s = &d.project.settings;
    assert_eq!(s.version_string().as_deref(), Some("25.2x17"));
    assert_eq!(s.revision, Some(9));
    assert_eq!(s.audio_sample_rate.unwrap().value, Some(48000.0));
    assert_eq!(s.working_gamma.unwrap().value, Some(2.4));
    assert_eq!(s.time_display_type, Some(1));
    assert_eq!(s.feet_frames_per_foot, Some(40));
    assert_eq!(s.display_start_frame(), Some(1));
    assert!(s.linear_blending);
    assert!(!s.linearize_working_space);
    let cms = s.color_management.as_ref().unwrap();
    assert_eq!(cms.working_space.as_deref(), Some("sRGB"));
    assert_eq!(
        cms.ocio_configuration_file.as_deref(),
        Some("C:\\configs\\月😀.ocio")
    );
    assert_eq!(d.project.effect_names, ["ADBE Fill"]);
}
#[test]
fn optional_settings_and_legacy_header_keep_absence_distinct() {
    let d = parse(b"RIFX\0\0\0\x04Egg!").unwrap();
    assert!(d.project.settings.audio_sample_rate.is_none());
    let mut h = vec![0; 32];
    h[8] = 1;
    h[12] = 25;
    h[13] = 16;
    h[14] = 2;
    let bytes = aep(vec![chunk(b"nhed", &h), chunk(b"adfr", &[0; 3])]);
    let d = parse(&bytes).unwrap();
    assert_eq!(d.project.settings.timecode_default_base, Some(25));
    assert_eq!(d.project.settings.frames_count_type, Some(2));
    assert!(
        d.project
            .diagnostics
            .iter()
            .any(|d| d.code == "incomplete-project-setting")
    );
}
#[test]
fn property_flags_ranges_and_separation_are_not_dimension_guesses() {
    let mut desc = vec![0; 124];
    desc[3] = 3;
    desc[5] = 9;
    desc[11] = 2;
    desc[0x3b] = 8;
    desc[0x44] = 1;
    let range = [1.0f64, 2.0, 3.0]
        .into_iter()
        .flat_map(f64::to_be_bytes)
        .collect::<Vec<_>>();
    let nested = property(
        "ADBE Position",
        &desc,
        vec![
            chunk(b"cdat", &range),
            chunk(b"tdsb", &[0, 0, 16, 3]),
            chunk(b"tdum", &range),
        ],
    );
    let bytes = project(vec![
        chunk(b"tdmn", b"ADBE Transform Group"),
        list(b"tdgp", nested),
    ]);
    let d = parse(&bytes).unwrap();
    let n = d.project.compositions[0].layers[0]
        .node("ADBE Position")
        .unwrap();
    let p = n.property.as_ref().unwrap();
    assert_eq!(n.depth, 1);
    assert_eq!(p.metadata.spatial, Some(true));
    assert!(p.spatial);
    assert_eq!(p.metadata.control_type, Some(18));
    assert_eq!(p.metadata.value_type, PropertyValueType::ThreeDSpatial);
    assert_eq!(p.metadata.dimensions_separated, Some(true));
    assert!(p.metadata.separation_leader);
    assert_eq!(p.metadata.locked_ratio, Some(true));
    assert_eq!(p.metadata.animated, Some(true));
    assert_eq!(
        p.metadata.min_value.as_ref().unwrap().numbers().unwrap()[2].value,
        Some(3.0)
    );
}
#[test]
fn integer_color_and_layer_reference_ranges() {
    let mut desc = vec![0; 124];
    desc[3] = 1;
    desc[0x3b] = 4;
    let mut props = property(
        "Integer",
        &desc,
        vec![
            chunk(b"cdat", &2.0f64.to_be_bytes()),
            chunk(b"tdum", &5u32.to_be_bytes()),
        ],
    );
    desc[3] = 4;
    desc[0x3b] = 1;
    let colors = [0.1f32, 0.2, 0.3, 1.0]
        .into_iter()
        .flat_map(f32::to_be_bytes)
        .collect::<Vec<_>>();
    props.extend(property(
        "Color",
        &desc,
        vec![chunk(b"cdat", &[0; 32]), chunk(b"tduM", &colors)],
    ));
    props.extend(property(
        "Layer",
        &desc,
        vec![
            chunk(b"cdat", &[0; 32]),
            chunk(b"tdpi", &27u32.to_be_bytes()),
        ],
    ));
    let bytes = project(props);
    let d = parse(&bytes).unwrap();
    let l = &d.project.compositions[0].layers[0];
    assert_eq!(
        scalar(&l.property("Integer").unwrap().metadata.min_value),
        Some(5.0)
    );
    assert_eq!(
        l.property("Color").unwrap().metadata.value_type,
        PropertyValueType::Color
    );
    assert_eq!(
        l.property("Color")
            .unwrap()
            .metadata
            .max_value
            .as_ref()
            .unwrap()
            .numbers()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(l.property("Layer").unwrap().metadata.control_type, Some(0));
    assert_eq!(l.property("Layer").unwrap().layer_reference_id, Some(27));
}
fn marker() -> Vec<u8> {
    let mut h = vec![0; 52];
    set32(&mut h, 8, 1);
    set32(&mut h, 16, 16);
    let mut d = vec![0; 16];
    set32(&mut d, 0, 12000);
    let mut nm = vec![0; 20];
    set32(&mut nm, 8, 1200);
    nm[3] = 3;
    nm[16] = 6;
    list(
        b"mrst",
        vec![
            list(
                b"tdbs",
                vec![list(b"list", vec![chunk(b"lhd3", &h), chunk(b"ldat", &d)])],
            ),
            list(
                b"mrky",
                vec![list(
                    b"Nmrd",
                    vec![
                        chunk(b"NmHd", &nm),
                        chunk(b"Utf8", b"note"),
                        chunk(b"Utf8", b"chapter"),
                        chunk(b"Utf8", b"url"),
                        chunk(b"Utf8", b"frame"),
                        chunk(b"Utf8", b"cue"),
                    ],
                )],
            ),
        ],
    )
}
#[test]
fn composition_and_layer_markers_share_limits_and_retain_uncertainty() {
    let bytes = aep(vec![list(
        b"Fold",
        vec![comp(vec![
            list(
                b"SecL",
                vec![
                    chunk(b"Utf8", b"Markers"),
                    list(b"tdgp", vec![chunk(b"tdmn", b"ADBE Marker"), marker()]),
                ],
            ),
            layer(vec![chunk(b"tdmn", b"ADBE Marker"), marker()], [0, 0, 1]),
        ])],
    )]);
    let d = parse(&bytes).unwrap();
    let c = &d.project.compositions[0];
    assert_eq!(c.markers[0].time_seconds, 0.5);
    assert_eq!(c.markers[0].comment, "note");
    assert_eq!(c.markers[0].duration_seconds, Some(2.0));
    assert!(!c.markers[0].duration_layout_verified);
    assert_eq!(c.layers[0].markers[0].cue_point_name, "cue");
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
fn guides_and_layer_alternate_source_decode_without_a_template() {
    let mut g = vec![0; 16];
    set32(&mut g, 0, 2);
    g[8..16].copy_from_slice(&123.5f64.to_be_bytes());
    let overrides = vec![
        chunk(b"tdmn", b"ADBE Layer Overrides"),
        list(b"OvG2", vec![]),
        list(
            b"tdgp",
            vec![
                chunk(b"tdmn", b"ADBE Layer Source Alternate"),
                chunk(b"blsv", &1u32.to_be_bytes()),
                chunk(b"blsi", &42u32.to_be_bytes()),
            ],
        ),
    ];
    let bytes = aep(vec![list(
        b"Fold",
        vec![comp(vec![
            list(b"Gide", vec![list(b"list", vec![chunk(b"ldat", &g)])]),
            layer(overrides, [16, 0, 1]),
        ])],
    )]);
    let d = parse(&bytes).unwrap();
    let c = &d.project.compositions[0];
    assert_eq!(c.guides[0].position.value, Some(123.5));
    assert_eq!(d.bytes(c.guides[0].source).unwrap(), g);
    assert_eq!(c.resolution_factor, Some([2, 3]));
    assert_eq!(c.motion_blur_samples_per_frame, Some(16));
    assert_eq!(c.layers[0].alternate_source_id, Some(42));
    assert_eq!(
        c.layers[0].auto_orient,
        Some(AutoOrient::CharactersTowardCamera)
    );
}
fn pard(control: u8, body: &[u8]) -> Vec<u8> {
    let mut d = vec![0; 56];
    d[15] = control;
    d[16..22].copy_from_slice(b"Choice");
    d.extend(body);
    chunk(b"pard", &d)
}
#[test]
fn effect_definitions_and_enum_labels_come_from_the_project() {
    let mut body = vec![0; 12];
    set32(&mut body, 0, 2);
    set32(&mut body, 4, 3 * 65536);
    set32(&mut body, 8, 1);
    let text = chunk(b"Utf8", b"Red | Green | Blue"); // Embedded record, not an ordinary child LIST.
    let mut desc = vec![0; 124];
    desc[3] = 1;
    desc[0x3b] = 4;
    let effect = list(
        b"sspc",
        vec![
            list(
                b"parT",
                vec![
                    chunk(b"tdmn", b"Effect"),
                    pard(13, &[]),
                    chunk(b"tdmn", b"Custom-0001"),
                    pard(7, &body),
                    chunk(b"pdnm", &text),
                ],
            ),
            list(
                b"tdgp",
                property(
                    "Custom-0001",
                    &desc,
                    vec![chunk(b"cdat", &2.0f64.to_be_bytes())],
                ),
            ),
        ],
    );
    let bytes = project(vec![
        chunk(b"tdmn", b"ADBE Effect Parade"),
        list(b"tdgp", vec![chunk(b"tdmn", b"Custom"), effect]),
    ]);
    let d = parse(&bytes).unwrap();
    let l = &d.project.compositions[0].layers[0];
    let def = &l.effects()[0].effect_parameters[0];
    assert_eq!(def.enum_options, ["Red", "Green", "Blue"]);
    assert_eq!(def.control_type, 7);
    assert_eq!(scalar(&def.default_value), Some(1.0));
    assert_eq!(
        l.property("Custom-0001").unwrap().metadata.control_type,
        Some(7)
    );
    assert_eq!(
        d.bytes(def.source).unwrap().get(..4),
        Some(b"pard".as_slice())
    );
}
#[test]
fn footage_saved_metadata_and_unicode_alias_do_not_touch_the_filesystem() {
    let mut d = vec![0; 222];
    d[0x78] = 1;
    d[0xa0..0xa8].copy_from_slice(&44100.0f64.to_be_bytes());
    set32(&mut d, 0xac, 10);
    set32(&mut d, 0xb0, 25);
    let bytes = aep(vec![list(
        b"Fold",
        vec![item(
            7,
            vec![list(
                b"Pin ",
                vec![
                    chunk(b"sspc", &d),
                    chunk(b"opti", b"png!"),
                    chunk(
                        b"alas",
                        br#"{"fullpath":"Z:\\missing\\\u6708\ud83d\ude00.png"}"#,
                    ),
                ],
            )],
        )],
    )]);
    let d = parse(&bytes).unwrap();
    let f = &d.project.footage[0];
    assert_eq!(f.path.as_deref(), Some("Z:\\missing\\月😀.png"));
    assert_eq!(f.missing_at_save, Some(true));
    assert_eq!(f.audio_sample_rate.unwrap().value, Some(44100.0));
    assert_eq!(f.start_frame, Some(10));
}
#[test]
fn invalid_json_tables_and_effects_remain_raw_and_diagnostic() {
    for text in [
        r#"{"colorManagementSystem":01}"#,
        r#"{"colorManagementSystem":1,}"#,
        r#"{"colorManagementSystem":"\ud800"}"#,
        r#"{"colorManagementSystem":1e999}"#,
    ] {
        let bytes = aep(vec![chunk(b"Utf8", text.as_bytes())]);
        let d = parse(&bytes).unwrap();
        assert!(d.project.settings.color_management.is_none());
        assert!(
            d.project
                .diagnostics
                .iter()
                .any(|d| d.code == "invalid-color-settings")
        );
    }
    let bytes = project(vec![
        chunk(b"tdmn", b"Bad"),
        list(
            b"sspc",
            vec![list(
                b"parT",
                vec![
                    chunk(b"tdmn", b"header"),
                    pard(13, &[]),
                    chunk(b"tdmn", b"bad-param"),
                    chunk(b"pard", &[0; 4]),
                ],
            )],
        ),
    ]);
    let d = parse(&bytes).unwrap();
    let n = d.project.compositions[0].layers[0].node("Bad").unwrap();
    assert_eq!(n.effect_parameters[0].decode_status, DecodeStatus::Partial);
    assert!(d.bytes(n.effect_parameters[0].source).is_some());
}
#[test]
fn json_depth_limit_is_shared_with_other_nested_payloads() {
    let text = format!(
        "{{\"colorManagementSystem\":1,\"nested\":{}0{}}}",
        "[".repeat(12),
        "]".repeat(12)
    );
    let bytes = aep(vec![chunk(b"Utf8", text.as_bytes())]);
    let limits = Limits {
        max_depth: 4,
        ..Limits::default()
    };
    assert!(matches!(
        parse_with_limits(&bytes, &limits),
        Err(Error::ResourceLimit {
            resource: "JSON depth",
            ..
        })
    ));
}

#[test]
fn effect_numeric_control_layouts_and_unknown_payloads() {
    let mut table = vec![chunk(b"tdmn", b"header"), pard(13, &[])];
    let mut scalar_body = vec![0; 82];
    set32(&mut scalar_body, 0, (2.5 * 65536.0) as u32);
    scalar_body[76..78].copy_from_slice(&(-10i16).to_be_bytes());
    scalar_body[80..82].copy_from_slice(&50i16.to_be_bytes());
    let angle_body = (-65536i32).to_be_bytes().to_vec();
    let boolean_body = vec![0, 0, 0, 1, 0];
    let mut two_d_body = vec![0; 8];
    set32(&mut two_d_body, 0, 1280);
    set32(&mut two_d_body, 4, 2560);
    let mut slider_body = vec![0; 64];
    slider_body[..8].copy_from_slice(&0.75f64.to_be_bytes());
    slider_body[60..64].copy_from_slice(&100.0f32.to_be_bytes());
    let three_d_body = [0.5f64, 1.0, 1.5]
        .into_iter()
        .flat_map(f64::to_be_bytes)
        .collect::<Vec<_>>();
    for (name, control, body) in [
        ("Scalar", 2, scalar_body),
        ("Angle", 3, angle_body),
        ("Boolean", 4, boolean_body),
        ("Point", 6, two_d_body),
        ("Slider", 10, slider_body),
        ("Point3", 18, three_d_body),
        ("Unknown", 99, vec![0; 12]),
        ("Color", 5, vec![0; 32]),
    ] {
        table.push(chunk(b"tdmn", name.as_bytes()));
        table.push(pard(control, &body));
    }
    let bytes = project(vec![
        chunk(b"tdmn", b"Custom"),
        list(b"sspc", vec![list(b"parT", table)]),
    ]);
    let d = parse(&bytes).unwrap();
    let defs = &d.project.compositions[0].layers[0]
        .node("Custom")
        .unwrap()
        .effect_parameters;
    assert_eq!(scalar(&defs[0].last_value), Some(2.5));
    assert_eq!(defs[0].min_value.unwrap().value, Some(-10.0));
    assert_eq!(scalar(&defs[1].default_value), Some(-1.0));
    assert_eq!(scalar(&defs[2].default_value), Some(0.0));
    assert_eq!(
        defs[3].last_value.as_ref().unwrap().numbers().unwrap()[1].value,
        Some(20.0)
    );
    assert_eq!(defs[4].max_value.unwrap().value, Some(100.0));
    assert_eq!(
        defs[5].last_value.as_ref().unwrap().numbers().unwrap()[2].value,
        Some(768.0)
    );
    assert_eq!(defs[6].decode_status, DecodeStatus::Partial);
    assert_eq!(defs[7].decode_status, DecodeStatus::Partial);
}
#[test]
fn malformed_marker_and_guide_records_are_recoverable() {
    let mut header = vec![0; 52];
    set32(&mut header, 8, u32::MAX);
    set32(&mut header, 16, 16);
    let bad = list(
        b"mrst",
        vec![list(
            b"tdbs",
            vec![list(
                b"list",
                vec![chunk(b"lhd3", &header), chunk(b"ldat", &[0; 16])],
            )],
        )],
    );
    let bytes = aep(vec![list(
        b"Fold",
        vec![comp(vec![
            layer(vec![chunk(b"tdmn", b"ADBE Marker"), bad], [0, 0, 1]),
            list(b"Gide", vec![list(b"list", vec![chunk(b"ldat", &[0; 17])])]),
        ])],
    )]);
    let d = parse(&bytes).unwrap();
    assert!(d.project.compositions[0].layers[0].markers.is_empty());
    assert_eq!(d.project.compositions[0].guides.len(), 1);
    for code in ["invalid-markers", "incomplete-guides"] {
        assert!(d.project.diagnostics.iter().any(|d| d.code == code));
    }
}
#[test]
fn descriptor_no_value_and_path_expressions_keep_metadata() {
    let mut desc = vec![0; 124];
    desc[0x39] = 1;
    desc[0x77] = 1;
    let mut props = property("NoValue", &desc, vec![]);
    props.extend([
        chunk(b"tdmn", b"ADBE Mask Shape"),
        list(
            b"om-s",
            vec![list(
                b"tdbs",
                vec![chunk(b"tdb4", &desc), chunk(b"Utf8", b"value")],
            )],
        ),
    ]);
    let bytes = project(props);
    let d = parse(&bytes).unwrap();
    let l = &d.project.compositions[0].layers[0];
    assert_eq!(
        l.node("NoValue").unwrap().decode_status,
        DecodeStatus::Decoded
    );
    let p = l.property("ADBE Mask Shape").unwrap();
    assert_eq!(p.expression.as_deref(), Some("value"));
    assert_eq!(p.expression_enabled, Some(false));
}

#[test]
fn unknown_pard_type_does_not_erase_a_descriptor_type() {
    let mut desc = vec![0; 124];
    desc[3] = 1;
    desc[0x3b] = 8;
    let bytes = project(vec![
        chunk(b"tdmn", b"Custom"),
        list(
            b"sspc",
            vec![
                list(
                    b"parT",
                    vec![
                        chunk(b"tdmn", b"header"),
                        pard(13, &[]),
                        chunk(b"tdmn", b"Param"),
                        pard(15, &[]),
                    ],
                ),
                list(
                    b"tdgp",
                    property("Param", &desc, vec![chunk(b"cdat", &2.0f64.to_be_bytes())]),
                ),
            ],
        ),
    ]);
    let d = parse(&bytes).unwrap();
    let p = d.project.compositions[0].layers[0]
        .property("Param")
        .unwrap();
    assert_eq!(p.metadata.control_type, Some(2));
    assert_eq!(p.effect_definition.as_ref().unwrap().control_type, 15);
}

fn checkbox_table(value: u8) -> Vec<u8> {
    list(
        b"parT",
        vec![
            chunk(b"tdmn", b"Custom-0000"),
            pard(13, &[]),
            chunk(b"tdmn", b"Custom-0001"),
            pard(4, &[0, 0, 0, value, value]),
        ],
    )
}
fn catalog_entry(table: Vec<u8>) -> Vec<u8> {
    list(
        b"EfDf",
        vec![chunk(b"tdmn", b"Custom"), list(b"sspc", vec![table])],
    )
}
#[test]
fn effect_catalog_is_separate_from_instance_definitions_and_values() {
    let instance = list(b"sspc", vec![checkbox_table(1), list(b"tdgp", vec![])]);
    let elided = list(b"sspc", vec![list(b"parT", vec![]), list(b"tdgp", vec![])]);
    let bytes = aep(vec![
        list(b"EfdG", vec![catalog_entry(checkbox_table(0))]),
        list(
            b"Fold",
            vec![comp(vec![layer(
                vec![
                    chunk(b"tdmn", b"ADBE Effect Parade"),
                    list(
                        b"tdgp",
                        vec![
                            chunk(b"tdmn", b"Custom"),
                            instance,
                            chunk(b"tdmn", b"Custom"),
                            elided,
                        ],
                    ),
                ],
                [0, 0, 1],
            )])],
        ),
    ]);
    let d = parse(&bytes).unwrap();
    let global = d.project.effect_definition("Custom").unwrap();
    assert_eq!(scalar(&global.parameters[0].default_value), Some(0.0));
    assert!(d.project.effect_definition("Other").is_none());
    let instances = d.project.compositions[0].layers[0].effects();
    assert_eq!(
        scalar(&instances[0].effect_parameters[0].default_value),
        Some(1.0)
    );
    assert!(instances[1].effect_parameters.is_empty());
    assert!(instances[1].children.is_empty()); // No default value is materialized.
    assert_ne!(
        global.parameters[0].source,
        instances[0].effect_parameters[0].source
    );
    assert_eq!(
        d.bytes(global.source).unwrap().get(8..12),
        Some(b"sspc".as_slice())
    );
    assert_eq!(
        d.bytes(global.parameters[0].source).unwrap().get(..4),
        Some(b"pard".as_slice())
    );
}
#[test]
fn duplicate_effect_catalog_entries_remain_visible_but_lookup_is_ambiguous() {
    let bytes = aep(vec![list(
        b"EfdG",
        vec![
            catalog_entry(checkbox_table(0)),
            catalog_entry(checkbox_table(1)),
        ],
    )]);
    let d = parse(&bytes).unwrap();
    assert_eq!(d.project.effect_definitions.len(), 2);
    assert!(d.project.effect_definition("Custom").is_none());
}
#[test]
fn malformed_effect_catalog_retains_partial_definitions_and_diagnostics() {
    let bytes = aep(vec![list(
        b"EfdG",
        vec![
            list(b"EfDf", vec![chunk(b"tdmn", b"orphan")]),
            list(
                b"EfDf",
                vec![chunk(b"tdmn", b"missing"), list(b"sspc", vec![])],
            ),
            catalog_entry(list(
                b"parT",
                vec![
                    chunk(b"tdmn", b"header"),
                    pard(13, &[]),
                    chunk(b"tdmn", b"truncated"),
                    chunk(b"pard", &[0; 12]),
                    chunk(b"tdmn", b"no-pard"),
                ],
            )),
        ],
    )]);
    let d = parse(&bytes).unwrap();
    for code in [
        "invalid-effect-catalog",
        "missing-effect-parameter-table",
        "partial-effect-definition",
        "invalid-effect-definition",
    ] {
        assert!(
            d.project.diagnostics.iter().any(|v| v.code == code),
            "{code}"
        );
    }
    let partial = &d.project.effect_definition("Custom").unwrap().parameters[0];
    assert_eq!(partial.decode_status, DecodeStatus::Partial);
    assert!(partial.default_value.is_none());
    assert_eq!(d.bytes(partial.source).unwrap().len(), 20);
    assert!(parse(&bytes[..bytes.len() - 1]).is_err());
    assert!(matches!(
        parse_with_limits(
            &bytes,
            &Limits {
                max_allocated_bytes: 1,
                ..Limits::default()
            }
        ),
        Err(Error::ResourceLimit { .. })
    ));
}

#[test]
fn effect_and_group_enabled_bits_preserve_unknown_and_disabled_states() {
    for (data, expected) in [
        (vec![0, 0, 0, 0], Some(false)),
        (vec![0, 0, 0, 3], Some(true)),
        (vec![1], None),
    ] {
        let bytes = project(vec![
            chunk(b"tdmn", b"ADBE Effect Parade"),
            list(
                b"tdgp",
                vec![
                    chunk(b"tdsb", &data),
                    chunk(b"tdmn", b"Custom"),
                    list(b"sspc", vec![list(b"tdgp", vec![chunk(b"tdsb", &data)])]),
                ],
            ),
        ]);
        let d = parse(&bytes).unwrap();
        let layer = &d.project.compositions[0].layers[0];
        assert_eq!(layer.node("ADBE Effect Parade").unwrap().enabled, expected);
        assert_eq!(layer.effects()[0].enabled, expected);
    }
    let bytes = project(vec![chunk(b"tdmn", b"Custom"), list(b"sspc", vec![])]);
    let d = parse(&bytes).unwrap();
    assert_eq!(
        d.project.compositions[0].layers[0].properties[0].enabled,
        None
    );
}
