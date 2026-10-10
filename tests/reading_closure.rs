#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
use aep_reader::{
    DecodeStatus, Error, FrameRate, Limits, TextRange, Value, parse, parse_with_limits,
};

fn chunk(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = tag.to_vec();
    out.extend((data.len() as u32).to_be_bytes());
    out.extend(data);
    if data.len() % 2 != 0 {
        out.push(0);
    }
    out
}
fn container(tag: &[u8; 4], form: &[u8; 4], children: Vec<Vec<u8>>) -> Vec<u8> {
    let mut data = form.to_vec();
    for child in children {
        data.extend(child);
    }
    chunk(tag, &data)
}
fn list(form: &[u8; 4], children: Vec<Vec<u8>>) -> Vec<u8> {
    container(b"LIST", form, children)
}
fn set32(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}
fn project(props: Vec<Vec<u8>>) -> Vec<u8> {
    let mut item = vec![0; 64];
    item[..2].copy_from_slice(&4u16.to_be_bytes());
    set32(&mut item, 16, 3);
    let mut comp = vec![0; 204];
    set32(&mut comp, 8, 8000);
    set32(&mut comp, 0x9c, 24 * 65536);
    let mut layer = vec![0; 164];
    set32(&mut layer, 0, 7);
    set32(&mut layer, 0x28, 10);
    layer[0x27] = 1;
    container(
        b"RIFX",
        b"Egg!",
        vec![list(
            b"Fold",
            vec![list(
                b"Item",
                vec![
                    chunk(b"idta", &item),
                    chunk(b"Utf8", b"Demo"),
                    chunk(b"cdta", &comp),
                    list(
                        b"Layr",
                        vec![
                            chunk(b"ldta", &layer),
                            chunk(b"Utf8", b"Layer"),
                            list(b"tdgp", props),
                        ],
                    ),
                ],
            )],
        )],
    )
}
fn table(count: u32, stride: u32, times: &[u32]) -> Vec<u8> {
    let mut header = vec![0; 52];
    set32(&mut header, 8, count);
    set32(&mut header, 16, stride);
    let mut data = vec![0; times.len() * stride as usize];
    for (index, time) in times.iter().enumerate() {
        let offset = index * stride as usize;
        set32(&mut data, offset, *time);
        data[offset + 4] = 2;
        data[offset + 5] = 3;
    }
    list(
        b"list",
        vec![chunk(b"lhd3", &header), chunk(b"ldat", &data)],
    )
}
fn gradient(time_table: Option<Vec<u8>>) -> Vec<Vec<u8>> {
    let xml=b"<prop.map version='4'><prop.list><prop.pair><key>Gradient Color Data</key><prop.list/></prop.pair></prop.list></prop.map>";
    vec![
        chunk(b"tdmn", b"ADBE Vector Grad Colors"),
        list(
            b"GCst",
            vec![
                list(b"tdbs", time_table.into_iter().collect()),
                list(b"GCky", vec![chunk(b"Utf8", xml), chunk(b"Utf8", xml)]),
            ],
        ),
    ]
}
#[test]
fn gradient_times_preserve_ticks_interpolation_and_raw_record() {
    let bytes = project(gradient(Some(table(2, 64, &[4000, 12000]))));
    let doc = parse(&bytes).unwrap();
    let node = doc.project.compositions[0].layers[0]
        .node("ADBE Vector Grad Colors")
        .unwrap();
    let p = node.property.as_ref().unwrap();
    let first = p.gradient_snapshots[0].timing.as_ref().unwrap();
    assert_eq!(
        (
            first.time_ticks,
            first.time_seconds,
            first.in_interpolation,
            first.out_interpolation
        ),
        (4000, 0.5, 2, 3)
    );
    assert_eq!(doc.bytes(first.source).unwrap().len(), 64);
    assert_eq!(
        &doc.bytes(first.source).unwrap()[..4],
        &4000u32.to_be_bytes()
    );
    assert_eq!(
        p.gradient_snapshots[1]
            .timing
            .as_ref()
            .unwrap()
            .time_seconds,
        1.5
    );
    assert!(p.is_animated());
    assert_eq!(p.is_modified(), Some(true));
    assert_eq!(node.decode_status, DecodeStatus::Partial);
    assert!(
        doc.project
            .diagnostics
            .iter()
            .any(|d| d.code == "gradient-easing-unverified")
    );
}
#[test]
fn invalid_gradient_tables_never_invent_snapshot_associations() {
    for time_table in [
        table(1, 64, &[0]),
        table(2, 48, &[0, 8000]),
        table(2, 64, &[0]),
    ] {
        let bytes = project(gradient(Some(time_table)));
        let doc = parse(&bytes).unwrap();
        let p = doc.project.compositions[0].layers[0]
            .property("ADBE Vector Grad Colors")
            .unwrap();
        assert_eq!(p.gradient_snapshots.len(), 2);
        assert!(p.gradient_snapshots.iter().all(|s| s.timing.is_none()));
        assert!(
            doc.project
                .diagnostics
                .iter()
                .any(|d| d.code == "gradient-timing-unverified")
        );
    }
}
#[test]
fn gradient_keyframes_use_the_shared_cumulative_budget() {
    let mut props = gradient(Some(table(2, 64, &[0, 8000])));
    props.extend(gradient(Some(table(2, 64, &[0, 16000]))));
    let bytes = project(props);
    let limits = Limits {
        max_keyframes: 3,
        ..Limits::default()
    };
    assert!(matches!(
        parse_with_limits(&bytes, &limits),
        Err(Error::ResourceLimit {
            resource: "keyframe count",
            actual: 4,
            ..
        })
    ));
}
fn shape(bounds: [f32; 4], flag: u8) -> Vec<u8> {
    let mut header = vec![0; 24];
    header[3] = flag;
    for (i, n) in bounds.iter().enumerate() {
        header[4 + i * 4..8 + i * 4].copy_from_slice(&n.to_be_bytes());
    }
    let mut layout = vec![0; 52];
    set32(&mut layout, 8, 3);
    set32(&mut layout, 16, 8);
    let mut data = Vec::new();
    for point in [[0.25f32, 0.5], [0., 0.5], [0.5, 0.75]] {
        for n in point {
            data.extend(n.to_be_bytes());
        }
    }
    list(
        b"shap",
        vec![
            chunk(b"shph", &header),
            list(
                b"list",
                vec![chunk(b"lhd3", &layout), chunk(b"ldat", &data)],
            ),
        ],
    )
}
fn path(shapes: Vec<Vec<u8>>, times: Option<Vec<u8>>) -> Vec<Vec<u8>> {
    let mut children = vec![list(b"omks", shapes)];
    if let Some(t) = times {
        children.push(list(b"tdbs", vec![t]));
    }
    vec![
        chunk(b"tdmn", b"ADBE Vector Shape"),
        list(b"om-s", children),
    ]
}
#[test]
fn path_bounds_restore_anchors_and_both_control_points() {
    let bytes = project(path(vec![shape([-10., 20., 30., 100.], 0)], None));
    let doc = parse(&bytes).unwrap();
    let Some(Value::Path(path)) = &doc.project.compositions[0].layers[0]
        .property("ADBE Vector Shape")
        .unwrap()
        .static_value
    else {
        panic!("path");
    };
    assert_eq!(path.closed, Some(false));
    assert_eq!(path.vertices[0].anchor[0].value, Some(0.25));
    let vertex = path.layer_vertices().unwrap().next().unwrap();
    assert_eq!(vertex.anchor.map(|n| n.value), [Some(0.), Some(60.)]);
    assert_eq!(vertex.out_tangent.map(|n| n.value), [Some(-10.), Some(60.)]);
    assert_eq!(vertex.in_tangent.map(|n| n.value), [Some(10.), Some(80.)]);
    for bounds in [[f32::NAN, 0., 10., 10.], [10., 0., -10., 20.]] {
        let bytes = project(path_props(bounds));
        let doc = parse(&bytes).unwrap();
        let Some(Value::Path(p)) = &doc.project.compositions[0].layers[0]
            .property("ADBE Vector Shape")
            .unwrap()
            .static_value
        else {
            panic!("path");
        };
        assert!(p.layer_vertices().is_none());
    }
}
fn path_props(bounds: [f32; 4]) -> Vec<Vec<u8>> {
    path(vec![shape(bounds, 1)], None)
}
#[test]
fn mismatched_paths_keep_all_geometry_without_wrong_keyframe_values() {
    let bytes = project(path(
        vec![
            shape([0., 0., 40., 40.], 1),
            shape([10., 10., 150., 150.], 9),
        ],
        Some(table(1, 64, &[4000])),
    ));
    let doc = parse(&bytes).unwrap();
    let p = doc.project.compositions[0].layers[0]
        .property("ADBE Vector Shape")
        .unwrap();
    assert_eq!(p.keyframes.len(), 1);
    assert!(p.keyframes[0].value.is_none());
    assert_eq!(p.unmatched_path_snapshots.len(), 2);
    assert_eq!(
        p.unmatched_path_snapshots[1]
            .path
            .as_ref()
            .unwrap()
            .bounds
            .unwrap()[0]
            .value,
        Some(10.)
    );
    assert_eq!(
        &doc.bytes(p.unmatched_path_snapshots[1].source).unwrap()[8..12],
        b"shap"
    );
}
fn text_body(first_count: &str) -> Vec<u8> {
    let mut body = b"/1 << /1 [ << /0 << /0 (".to_vec();
    body.extend([0xfe, 0xff]);
    for unit in "😀A\r".encode_utf16() {
        body.extend(unit.to_be_bytes());
    }
    body.extend(b") ");
    body.extend(format!("/6 << /0 [ << /1 {first_count} /0 << /0 << /6 << /1 12 >> >> >> >> << /1 2 /0 << /0 << /6 << /1 24 >> >> >> >> ] >> /5 << /0 [ << /1 4 /0 << /0 << /5 << /0 2 >> >> >> >> ] >> >> >> ] >>").as_bytes());
    body
}
fn text_project(first_count: &str) -> Vec<u8> {
    project(vec![
        chunk(b"tdmn", b"ADBE Text Document"),
        list(b"btds", vec![list(b"btdk", vec![text_body(first_count)])]),
    ])
}
#[test]
fn text_ranges_count_utf16_and_never_split_a_surrogate_pair() {
    let bytes = text_project("2");
    let doc = parse(&bytes).unwrap();
    let text = doc.project.compositions[0].layers[0]
        .text_source
        .as_ref()
        .unwrap();
    assert_eq!(
        text.style_runs[0].range.unwrap().slice(&text.text),
        Some("😀")
    );
    assert_eq!(
        text.style_runs[1].range.unwrap().slice(&text.text),
        Some("A\r")
    );
    assert_eq!(
        text.paragraphs[0].range.unwrap().slice(&text.text),
        Some("😀A\r")
    );
    assert!(
        TextRange {
            utf16_start: 1,
            utf16_length: 1
        }
        .slice(&text.text)
        .is_none()
    );
    assert!(
        TextRange {
            utf16_start: usize::MAX,
            utf16_length: 1
        }
        .slice(&text.text)
        .is_none()
    );
    assert_eq!(
        TextRange {
            utf16_start: 4,
            utf16_length: 0
        }
        .slice(&text.text),
        Some("")
    );
}
#[test]
fn invalid_text_counts_do_not_shift_later_run_boundaries() {
    for count in ["-1", "1.5", "5"] {
        let bytes = text_project(count);
        let doc = parse(&bytes).unwrap();
        let text = doc.project.compositions[0].layers[0]
            .text_source
            .as_ref()
            .unwrap();
        assert!(text.style_runs.iter().all(|r| r.range.is_none()));
        assert_eq!(text.style_runs[1].font_size.unwrap().value, Some(24.));
    }
}
#[test]
fn frame_conversions_check_rate_overflow_and_rounding() {
    for invalid in [0., -1., f64::NAN, f64::INFINITY, f64::from_bits(1)] {
        assert!(FrameRate::new(invalid).is_none());
    }
    let rate = FrameRate::new(25.).unwrap();
    assert_eq!(rate.frames(0.02), Some(1));
    assert_eq!(rate.frames(-0.02), Some(-1));
    assert_eq!(rate.seconds(75), Some(3.));
    assert_eq!(rate.frame_duration(), 0.04);
    assert!(rate.frames(f64::MAX).is_none());
    assert!(rate.frames(f64::NAN).is_none());
    assert!(
        FrameRate::new(1.)
            .unwrap()
            .frames(i64::MAX as f64)
            .is_none()
    );
}
#[test]
fn direct_paths_parents_and_raw_queries_preserve_locality_and_bounds() {
    let props = vec![
        chunk(b"tdmn", b"Group"),
        list(
            b"tdgp",
            vec![chunk(b"tdmn", b"Leaf"), list(b"tdbs", vec![])],
        ),
    ];
    let bytes = project(props);
    let doc = parse(&bytes).unwrap();
    let layer = &doc.project.compositions[0].layers[0];
    let node = layer.node_by_path(&["Group", "Leaf"]).unwrap();
    assert_eq!(layer.parent_node_of(node).unwrap().match_name, "Group");
    assert!(layer.node_by_path(&["Leaf"]).is_none());
    assert!(layer.node_by_path(&[]).is_none());
    let other = parse(&bytes).unwrap();
    assert!(
        layer
            .parent_node_of(
                other.project.compositions[0].layers[0]
                    .node("Leaf")
                    .unwrap()
            )
            .is_none()
    );
    assert_eq!(
        doc.project.containing_composition(layer).unwrap().name,
        "Demo"
    );
    assert!(
        doc.project
            .containing_composition(&other.project.compositions[0].layers[0])
            .is_none()
    );
    assert_eq!(
        doc.project
            .composition_by_name("Demo")
            .unwrap()
            .layer_by_name("Layer")
            .unwrap()
            .id,
        7
    );
    assert_eq!(doc.root.find_all(b"tdmn").count(), 2);
    assert_eq!(doc.root.find_all_lists(b"tdgp").count(), 2);
    assert_eq!(doc.root.u32(0), Some(u32::from_be_bytes(*b"LIST")));
    assert!(doc.root.u16(usize::MAX).is_none());
}

#[test]
fn standalone_payload_reads_have_the_same_limits_and_reject_execution_syntax() {
    let payload = list(b"btdk", vec![text_body("2")]);
    let document = aep_reader::parse_text_document(&payload).unwrap();
    assert_eq!(document.text, "😀A\r");
    let limits = Limits {
        max_input_bytes: 8,
        ..Limits::default()
    };
    assert!(matches!(
        aep_reader::parse_text_document_with_limits(&payload, &limits),
        Err(Error::ResourceLimit {
            resource: "input bytes",
            ..
        })
    ));
    assert!(
        aep_reader::parse_gradient_xml(
            b"<!DOCTYPE x [<!ENTITY e SYSTEM 'file:///never-opened'>]><prop.map/>"
        )
        .is_err()
    );
    assert!(matches!(
        aep_reader::parse_gradient_xml_with_limits(b"<prop.map/>", &limits),
        Err(Error::ResourceLimit {
            resource: "input bytes",
            ..
        })
    ));
    assert!(
        aep_reader::parse_text_document(&list(b"btdk", vec![b"(secret) run".to_vec()])).is_err()
    );
}

#[test]
fn cubic_segment_controls_are_reassembled_per_vertex() {
    let mut header = vec![0; 24];
    header[3] = 1;
    header[12..16].copy_from_slice(&100f32.to_be_bytes());
    header[16..20].copy_from_slice(&100f32.to_be_bytes());
    let mut layout = vec![0; 52];
    set32(&mut layout, 8, 6);
    set32(&mut layout, 16, 8);
    // Each cubic segment starts at its anchor, then outgoing_i, incoming_(i+1).
    let pairs = [
        [0.1f32, 0.2],
        [0.2, 0.2],
        [0.2, 0.3],
        [0.3, 0.4],
        [0.4, 0.5],
        [0.05, 0.2],
    ];
    let data = pairs
        .into_iter()
        .flatten()
        .flat_map(f32::to_be_bytes)
        .collect::<Vec<_>>();
    let shp = list(
        b"shap",
        vec![
            chunk(b"shph", &header),
            list(
                b"list",
                vec![chunk(b"lhd3", &layout), chunk(b"ldat", &data)],
            ),
        ],
    );
    let bytes = project(path(vec![shp], None));
    let doc = parse(&bytes).unwrap();
    let Some(Value::Path(p)) = &doc.project.compositions[0].layers[0]
        .property("ADBE Vector Shape")
        .unwrap()
        .static_value
    else {
        panic!("path");
    };
    let points = p.layer_vertices().unwrap().collect::<Vec<_>>();
    for (actual, expected) in [
        (points[0].in_tangent, [5., 20.]),
        (points[0].out_tangent, [20., 20.]),
        (points[1].in_tangent, [20., 30.]),
        (points[1].out_tangent, [40., 50.]),
    ] {
        for (a, e) in actual.iter().zip(expected) {
            assert!((a.value.unwrap() - e).abs() < 1e-4);
        }
    }
}
