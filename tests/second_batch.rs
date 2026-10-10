#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
use aep_reader::{DecodeStatus, Error, FormatOptions, Limits, ShapeKind, parse, parse_with_limits};

fn chunk(id: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = id.to_vec();
    out.extend((data.len() as u32).to_be_bytes());
    out.extend(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
    out
}
fn container(id: &[u8; 4], form: &[u8; 4], children: Vec<Vec<u8>>) -> Vec<u8> {
    let mut data = form.to_vec();
    for child in children {
        data.extend(child);
    }
    chunk(id, &data)
}
fn list(form: &[u8; 4], children: Vec<Vec<u8>>) -> Vec<u8> {
    container(b"LIST", form, children)
}
fn aep(children: Vec<Vec<u8>>) -> Vec<u8> {
    container(b"RIFX", b"Egg!", children)
}
fn set16(data: &mut [u8], offset: usize, value: u16) {
    data[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}
fn set32(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}
fn comp(extra: Vec<Vec<u8>>) -> Vec<u8> {
    let mut id = vec![0; 64];
    set16(&mut id, 0, 4);
    set32(&mut id, 16, 3);
    let mut data = vec![0; 204];
    set32(&mut data, 8, 8000);
    set32(&mut data, 0x9c, 24 * 65536);
    let mut children = vec![chunk(b"idta", &id), chunk(b"cdta", &data)];
    children.extend(extra);
    list(b"Item", children)
}
fn project(props: Vec<Vec<u8>>) -> Vec<u8> {
    let mut data = vec![0; 164];
    set32(&mut data, 0, 7);
    aep(vec![list(
        b"Fold",
        vec![comp(vec![list(
            b"Layr",
            vec![chunk(b"ldta", &data), list(b"tdgp", props)],
        )])],
    )])
}
fn text(body: &[u8]) -> Vec<u8> {
    project(vec![
        chunk(b"tdmn", b"ADBE Text Document"),
        list(b"btds", vec![list(b"btdk", vec![body.to_vec()])]),
    ])
}

#[test]
fn advanced_text_retains_styles_axes_paragraphs_and_missing_fields() {
    let body=br#"/0 << /1 << /0 [ << /0 << /0 << /0 (VariableFont) /4 [26214400 4915200] >> >> >> ] >> /8 << /0 [ << /0 << /1 << /0 [-10 -20 100 -20 100 80 -10 80] >> >> >> ] >> >>
    /1 << /1 [ << /0 << /0 (AB\r) /5 << /0 [ << /0 << /0 << /5 << /0 2 /1 7.5 /2 10 /3 20 /4 3 /5 4 /8 1 /9 false /21 true /33 1 >> >> >> >> ] >>
    /6 << /0 [ << /0 << /0 << /6 << /0 0 /1 48 /2 true /3 false /4 false /5 60 /6 0.8 /7 1.2 /8 150 /9 -2 /11 0 /12 2 /13 1 /36 25 /52 true /53 << /0 << /1 [0.5 1 0.2 0.3] >> >> /54 << /0 << /1 [1 0 0 1] >> >> /57 true /58 false /62 2 /63 8 /70 1 >> >> >> >> ] >>
    /7 -45 /8 << /0 [ << /0 << /0 -45 >> >> << /0 << /0 120 >> >> << /0 << >> >> ] >> >> >> ] >>"#;
    let bytes = text(body);
    let doc = parse(&bytes).unwrap();
    let t = doc.project.compositions[0].layers[0]
        .text_source
        .as_ref()
        .unwrap();
    assert_eq!(t.text, "AB\r");
    assert_eq!(t.fonts, ["VariableFont"]);
    assert_eq!(t.font_axes[0][0].value, Some(400.0));
    assert_eq!(t.font_axes[0][1].value, Some(75.0));
    let r = &t.style_runs[0];
    assert_eq!(r.faux_bold, Some(true));
    assert_eq!(r.faux_italic, Some(false));
    assert_eq!(r.stroke_over_fill, Some(false));
    assert_eq!(r.caps_option.unwrap().value, Some(2.0));
    assert_eq!(r.all_caps(), Some(true));
    assert_eq!(r.small_caps(), Some(false));
    assert_eq!(r.superscript(), Some(true));
    assert_eq!(r.subscript(), Some(false));
    assert_eq!(r.font_name(t), Some("VariableFont"));
    assert_eq!(r.stroke_width.unwrap().value, Some(8.0));
    assert_eq!(r.fill_argb.unwrap()[0].value, Some(0.5));
    assert_eq!(t.paragraphs[0].first_line_indent.unwrap().value, Some(7.5));
    assert_eq!(t.paragraphs[0].auto_hyphenate, Some(false));
    assert_eq!(t.manual_kerning.len(), 2);
    assert_eq!(t.manual_kerning[1].value, Some(120.0));
    assert_eq!(
        t.box_bounds.unwrap().map(|n| n.value),
        [Some(-10.0), Some(-20.0), Some(100.0), Some(80.0)]
    );
    let bytes=text(b"/1 << /1 [ << /0 << /0 (x) /6 << /0 [ << /0 << /0 << /6 << /1 12 >> >> >> >> ] >> >> >> ] >>");
    let doc = parse(&bytes).unwrap();
    let t = doc.project.compositions[0].layers[0]
        .text_source
        .as_ref()
        .unwrap();
    assert_eq!(t.style_runs[0].faux_bold, None);
    assert_eq!(t.style_runs[0].stroke_over_fill, None);
    assert_eq!(t.style_runs[0].all_caps(), None);
    assert_eq!(t.style_runs[0].font_name(t), None);
    assert!(t.box_bounds.is_none());
}
fn gradient_xml(color: &str) -> String {
    format!(
        "<?xml version='1.0'?><prop.map version='4'><prop.list><prop.pair><key>Gradient Color Data</key><prop.list><prop.pair><key>Color Stops</key><prop.list><prop.pair><key>Stops List</key><prop.list><prop.pair><key>Stop-0</key><prop.list><prop.pair><key>Stops Color</key><array><array.type><float/></array.type><float>0</float><float>0.25</float><float>{color}</float><float>0.5</float><float>1</float><float>1</float></array></prop.pair></prop.list></prop.pair></prop.list></prop.pair></prop.list></prop.pair></prop.list></prop.pair></prop.list></prop.map>"
    )
}
fn gradient(xml: Vec<Vec<u8>>) -> Vec<u8> {
    project(vec![
        chunk(b"tdmn", b"ADBE Vector Grad Colors"),
        list(
            b"GCst",
            vec![
                list(b"tdbs", vec![]),
                list(
                    b"GCky",
                    xml.into_iter().map(|x| chunk(b"Utf8", &x)).collect(),
                ),
            ],
        ),
    ])
}
#[test]
fn gradient_preserves_every_snapshot_without_inventing_animation_times() {
    let x = gradient_xml("0.75");
    let y = gradient_xml("0.1");
    let bytes = gradient(vec![x.into_bytes(), y.into_bytes()]);
    let d = parse(&bytes).unwrap();
    let n = d.project.compositions[0].layers[0]
        .node("ADBE Vector Grad Colors")
        .unwrap();
    let p = n.property.as_ref().unwrap();
    assert_eq!(n.decode_status, DecodeStatus::Partial);
    assert_eq!(p.gradient_snapshots.len(), 2);
    assert!(p.keyframes.is_empty());
    assert!(p.static_value.is_none());
    assert_eq!(
        p.gradient_snapshots[1]
            .gradient
            .as_ref()
            .unwrap()
            .color_stops[0]
            .color[0]
            .value,
        Some(0.1)
    );
    assert!(
        d.project
            .diagnostics
            .iter()
            .any(|d| d.code == "gradient-timing-unverified")
    );
    assert_eq!(
        d.bytes(p.gradient_snapshots[0].source).unwrap().get(..4),
        Some(b"Utf8".as_slice())
    );
}
#[test]
fn malformed_and_external_entity_gradients_remain_diagnostic() {
    let inputs = [
        "<!DOCTYPE prop.map [<!ENTITY x SYSTEM 'file:///not-read'>]><prop.map/>",
        "<prop.map><prop.list></prop.map>",
        "<prop.map><key>&x;</key></prop.map>",
        "<prop.map version='4'junk='x'/>",
        "<prop.map/><ignored/>",
    ];
    for input in inputs {
        let bytes = gradient(vec![input.as_bytes().to_vec()]);
        let d = parse(&bytes).unwrap();
        let p = d.project.compositions[0].layers[0]
            .property("ADBE Vector Grad Colors")
            .unwrap();
        assert!(p.gradient_snapshots[0].gradient.is_none());
        assert!(
            d.project
                .diagnostics
                .iter()
                .any(|d| d.code == "unsupported-gradient-xml")
        );
    }
    let xml = gradient_xml("0").replace("Stop-0", "Stop-2");
    let bytes = gradient(vec![xml.into_bytes()]);
    let d = parse(&bytes).unwrap();
    assert!(
        d.project.compositions[0].layers[0]
            .property("ADBE Vector Grad Colors")
            .unwrap()
            .gradient_snapshots[0]
            .gradient
            .is_none()
    );
}
#[test]
fn gradient_depth_and_allocation_share_public_limits() {
    let xml = format!("{}{}", "<n>".repeat(25), "</n>".repeat(25));
    let bytes = gradient(vec![xml.into_bytes()]);
    let limits = Limits {
        max_depth: 20,
        ..Limits::default()
    };
    assert!(matches!(
        parse_with_limits(&bytes, &limits),
        Err(Error::ResourceLimit {
            resource: "gradient XML depth",
            ..
        })
    ));
    let bytes = gradient(vec![gradient_xml("0.5").into_bytes(); 20]);
    let limits = Limits {
        max_allocated_bytes: 8192,
        ..Limits::default()
    };
    assert!(matches!(
        parse_with_limits(&bytes, &limits),
        Err(Error::ResourceLimit {
            resource: "decoded allocation bytes",
            ..
        })
    ));
}
#[test]
fn essential_graphics_preserves_order_names_and_unknown_type_codes() {
    let ctrl = |id: &[u8], kind: &[u8], name: &[u8]| {
        list(
            b"CCtl",
            vec![
                chunk(b"Utf8", id),
                chunk(b"CTyp", kind),
                list(
                    b"CpS2",
                    vec![chunk(b"Utf8", name), chunk(b"Utf8", b"zh_CN")],
                ),
            ],
        )
    };
    let bytes = aep(vec![list(
        b"Fold",
        vec![comp(vec![list(
            b"CIF3",
            vec![
                list(b"CpS2", vec![chunk(b"Utf8", "月离".as_bytes())]),
                ctrl(b"second", &500u32.to_be_bytes(), b"B"),
                ctrl(b"first", &[0, 1], b"A"),
            ],
        )])],
    )]);
    let d = parse(&bytes).unwrap();
    let eg = d.project.compositions[0]
        .essential_graphics
        .as_ref()
        .unwrap();
    assert_eq!(eg.template_name.as_deref(), Some("月离"));
    assert_eq!(eg.controllers[0].uuid.as_deref(), Some("second"));
    assert_eq!(eg.controllers[0].type_code, Some(500));
    assert_eq!(eg.controllers[1].type_code, None);
    assert!(
        d.project
            .diagnostics
            .iter()
            .any(|d| d.code == "incomplete-essential-graphics-controller")
    );
    assert!(d.bytes(eg.source).is_some());
    let bytes = aep(vec![list(b"Fold", vec![comp(vec![list(b"CIF2", vec![])])])]);
    let d = parse(&bytes).unwrap();
    assert!(d.project.compositions[0].essential_graphics.is_none());
    assert!(
        d.project
            .diagnostics
            .iter()
            .any(|d| d.code == "unsupported-essential-graphics-version")
    );
}
fn table(data: &[u8], count: u32) -> Vec<u8> {
    let mut h = vec![0; 20];
    set32(&mut h, 8, count);
    list(b"list", vec![chunk(b"lhd3", &h), chunk(b"ldat", data)])
}
fn output(format: Vec<u8>, name: &[u8]) -> Vec<Vec<u8>> {
    let mut r = vec![0; 114];
    r[4..8].copy_from_slice(b"test");
    r[26..30].copy_from_slice(b"JPEG");
    set16(&mut r, 36, 1920);
    set16(&mut r, 40, 1080);
    r[71] = 24;
    r[100..108].copy_from_slice(&48000.0f64.to_be_bytes());
    r[111] = 16;
    r[113] = 2;
    let mut out = vec![
        chunk(b"Roou", &r),
        list(
            b"Als2",
            vec![chunk(b"alas", br#"{"fullpath":"D:\\output\\\u6708.mov"}"#)],
        ),
        chunk(b"Utf8", name),
        chunk(b"Utf8", b"[compName].[fileextension]"),
    ];
    if !format.is_empty() {
        out.push(format);
    }
    out
}
fn queue(rs: Vec<u8>, om: Vec<u8>, outputs: Vec<Vec<u8>>) -> Vec<u8> {
    aep(vec![list(
        b"LRdr",
        vec![
            table(&rs, 1),
            list(
                b"LItm",
                vec![
                    chunk(b"RCom", &chunk(b"Utf8", b"Review")),
                    table(&om, 2),
                    list(b"LOm ", outputs),
                ],
            ),
        ],
    )])
}
fn render_record() -> Vec<u8> {
    let mut d = vec![0; 2246];
    set32(&mut d, 8, 3);
    set32(&mut d, 12, 8);
    set32(&mut d, 20, 5);
    set32(&mut d, 24, 2);
    set32(&mut d, 28, 15);
    set32(&mut d, 32, 2);
    set16(&mut d, 2148, 2);
    set16(&mut d, 0x38, 65535);
    d[0x5a..0x5e].copy_from_slice(b"Best");
    d
}
#[test]
fn render_queue_pairs_output_records_and_preserves_exact_time_ratios() {
    let mut oms = vec![0; 256];
    oms[7] = 0xd8;
    oms[19] = 1;
    oms[27] = 1;
    oms[31] = 1;
    set16(&mut oms, 32, 9);
    oms[128 + 19] = 2;
    let mut jpg = vec![0; 58];
    jpg[..4].copy_from_slice(b"JPEG");
    set16(&mut jpg, 52, 93);
    let mut outputs = output(chunk(b"Ropt", &jpg), b"First");
    outputs.extend(output(Vec::new(), b"Second"));
    let bytes = queue(render_record(), oms, outputs);
    let d = parse(&bytes).unwrap();
    let q = d.project.render_queue.as_ref().unwrap();
    let item = &q.items[0];
    let s = item.settings.as_ref().unwrap();
    assert_eq!(s.quality, 65535);
    assert_eq!(s.time_span_start.value(), Some(2.5));
    assert_eq!(s.time_span_duration.value(), Some(7.5));
    assert_eq!(item.comment.as_deref(), Some("Review"));
    assert_eq!(item.output_modules.len(), 2);
    let first = &item.output_modules[0];
    assert_eq!(first.full_path.as_deref(), Some("D:\\output\\月.mov"));
    assert_eq!(first.settings.as_ref().unwrap().crop_top, 9);
    assert_eq!(
        item.output_modules[1].settings.as_ref().unwrap().channels,
        2
    );
    assert!(matches!(
        first.format_options,
        Some(FormatOptions::Jpeg { quality: 93 })
    ));
    assert_eq!(d.bytes(s.source).unwrap().len(), 2246);
    assert_eq!(
        d.bytes(first.settings.as_ref().unwrap().source)
            .unwrap()
            .len(),
        128
    );
}
#[test]
fn truncated_queue_records_and_bad_comments_do_not_fabricate_settings() {
    let bytes = queue(
        vec![0; 2245],
        vec![0; 127],
        vec![chunk(b"Roou", &[0; 113]), chunk(b"Ropt", b"bad!")],
    );
    let d = parse(&bytes).unwrap();
    let item = &d.project.render_queue.as_ref().unwrap().items[0];
    assert!(item.settings.is_none());
    assert!(item.output_modules[0].settings.is_none());
    assert!(item.output_modules[0].options.is_none());
    assert!(item.output_modules[0].format_options.is_none());
    for code in [
        "incomplete-render-settings-table",
        "missing-render-settings",
        "incomplete-output-options",
        "unsupported-output-format-options",
    ] {
        assert!(d.project.diagnostics.iter().any(|d| d.code == code));
    }
    let bytes = aep(vec![list(
        b"LRdr",
        vec![list(
            b"LItm",
            vec![
                chunk(b"RCom", b"Utf8\0\0\0\xffx"),
                list(b"LOm ", vec![]),
                table(&[], 0),
            ],
        )],
    )]);
    let d = parse(&bytes).unwrap();
    assert!(d.project.render_queue.as_ref().unwrap().items.is_empty());
    for code in [
        "invalid-render-comment",
        "orphan-output-modules",
        "orphan-render-item",
    ] {
        assert!(d.project.diagnostics.iter().any(|d| d.code == code));
    }
}
#[test]
fn typed_shape_views_keep_duplicate_group_boundaries_and_unknown_nodes() {
    let size = |value: f64| {
        vec![
            chunk(b"tdmn", b"ADBE Vector Rect Size"),
            list(
                b"tdbs",
                vec![
                    chunk(b"tdb4", &[0, 0, 0, 2]),
                    chunk(
                        b"cdat",
                        &[value.to_be_bytes(), 20.0f64.to_be_bytes()].concat(),
                    ),
                ],
            ),
        ]
    };
    let mut children = vec![
        chunk(b"tdmn", b"ADBE Vector Shape - Rect"),
        list(b"tdgp", size(10.0)),
        chunk(b"tdmn", b"ADBE Vector Shape - Rect"),
        list(b"tdgp", size(30.0)),
    ];
    children.extend([chunk(b"tdmn", b"ADBE New Shape"), list(b"tdgp", vec![])]);
    let bytes = project(vec![
        chunk(b"tdmn", b"ADBE Root Vectors Group"),
        list(b"tdgp", children),
    ]);
    let d = parse(&bytes).unwrap();
    let root = d.project.compositions[0].layers[0].shapes().unwrap();
    assert_eq!(root.kind(), ShapeKind::Root);
    let shapes: Vec<_> = root.children().collect();
    assert_eq!(shapes.len(), 3);
    assert_eq!(shapes[0].kind(), ShapeKind::Rectangle);
    assert_eq!(shapes[2].kind(), ShapeKind::Unknown);
    assert!(root.size().is_none());
    assert_eq!(
        shapes[0]
            .size()
            .unwrap()
            .static_value
            .as_ref()
            .unwrap()
            .numbers()
            .unwrap()[0]
            .value,
        Some(10.0)
    );
    assert_eq!(
        shapes[1]
            .size()
            .unwrap()
            .static_value
            .as_ref()
            .unwrap()
            .numbers()
            .unwrap()[0]
            .value,
        Some(30.0)
    );
    assert!(std::ptr::eq(shapes[0].node, &root.node.children[0]));
}

#[test]
fn known_output_formats_are_decoded_and_truncated_variants_reported() {
    for (code, size) in [
        (b"sDPX", 47),
        (b"JPEG", 58),
        (b"oEXR", 17),
        (b"TPIC", 83),
        (b"TIF ", 602),
        (b"png!", 34),
    ] {
        let mut data = vec![0; size];
        data[..4].copy_from_slice(code);
        match code {
            b"sDPX" => {
                set16(&mut data, 14, 95);
                set16(&mut data, 16, 685);
                data[34..42].copy_from_slice(&0.6f64.to_be_bytes());
            }
            b"JPEG" => set16(&mut data, 52, 88),
            b"oEXR" => {
                data[14] = 4;
                data[15] = 1;
            }
            b"TPIC" => {
                data[77] = 32;
                data[82] = 1;
            }
            b"TIF " => {
                data[600] = 1;
                data[601] = 1;
            }
            b"png!" => {
                set32(&mut data, 18, 640);
                set32(&mut data, 22, 480);
                set16(&mut data, 28, 16);
                set32(&mut data, 30, 9);
            }
            _ => unreachable!(),
        }
        let bytes = queue(
            render_record(),
            vec![0; 256],
            output(chunk(b"Ropt", &data), b"Output"),
        );
        let doc = parse(&bytes).unwrap();
        let f = doc.project.render_queue.as_ref().unwrap().items[0].output_modules[0]
            .format_options
            .as_ref()
            .unwrap();
        match (code, f) {
            (
                b"sDPX",
                FormatOptions::Cineon {
                    black_point: 95,
                    white_point: 685,
                    gamma,
                    ..
                },
            ) => assert_eq!(gamma.value, Some(0.6)),
            (b"JPEG", FormatOptions::Jpeg { quality: 88 }) => {}
            (
                b"oEXR",
                FormatOptions::OpenExr {
                    compression: 4,
                    float_32: true,
                    ..
                },
            ) => {}
            (
                b"TPIC",
                FormatOptions::Targa {
                    bits_per_pixel: 32,
                    rle_compression: true,
                },
            ) => {}
            (
                b"TIF ",
                FormatOptions::Tiff {
                    ibm_pc_byte_order: true,
                    lzw_compression: true,
                },
            ) => {}
            (
                b"png!",
                FormatOptions::Png {
                    width: 640,
                    height: 480,
                    bit_depth: 16,
                    compression: 9,
                },
            ) => {}
            _ => panic!("unexpected format {f:?}"),
        }
        data.pop();
        let bytes = queue(
            render_record(),
            vec![0; 256],
            output(chunk(b"Ropt", &data), b"Output"),
        );
        let doc = parse(&bytes).unwrap();
        assert!(
            doc.project.render_queue.as_ref().unwrap().items[0].output_modules[0]
                .format_options
                .is_none()
        );
        assert!(
            doc.project
                .diagnostics
                .iter()
                .any(|d| d.code == "unsupported-output-format-options")
        );
    }
}

#[test]
fn queue_time_resolution_requires_valid_sources_and_divisors() {
    let bytes = queue(render_record(), vec![0; 256], output(Vec::new(), b"Test"));
    let mut doc = parse(&bytes).unwrap();
    let s = doc.project.render_queue.as_mut().unwrap().items[0]
        .settings
        .as_mut()
        .unwrap();
    assert_eq!(s.resolved_time_span(None), Some((2.5, 7.5)));
    s.time_span_start.denominator = 0;
    assert!(s.resolved_time_span(None).is_none());
    s.time_span_source = 7;
    assert!(s.resolved_time_span(None).is_none());
    s.time_span_source = 0;
    assert!(s.resolved_time_span(None).is_none());
    let bytes = aep(vec![list(b"Fold", vec![comp(vec![])])]);
    let mut cdoc = parse(&bytes).unwrap();
    let c = &mut cdoc.project.compositions[0];
    c.duration_seconds = Some(10.0);
    c.work_area_start = Some(aep_reader::Ratio {
        numerator: 2,
        denominator: 1,
    });
    c.work_area_end = Some(aep_reader::Ratio {
        numerator: 8,
        denominator: 1,
    });
    assert_eq!(s.resolved_time_span(Some(c)), Some((0.0, 10.0)));
    s.time_span_source = 1;
    assert_eq!(s.resolved_time_span(Some(c)), Some((2.0, 6.0)));
    c.work_area_uses_duration = true;
    assert_eq!(s.resolved_time_span(Some(c)), Some((2.0, 8.0)));
    c.id = 4;
    assert!(s.resolved_time_span(Some(c)).is_none());
}
