#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
use aep_reader::{
    DefaultOrigin, FootageSourceRef, LayerKind, NameSource, Number, Property, PropertyUnit, Value,
    known_property_units, parse, property_unit,
};

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
    for c in children {
        data.extend(c);
    }
    chunk(id, &data)
}
fn list(form: &[u8; 4], children: Vec<Vec<u8>>) -> Vec<u8> {
    container(b"LIST", form, children)
}
fn set32(d: &mut [u8], o: usize, n: u32) {
    d[o..o + 4].copy_from_slice(&n.to_be_bytes());
}
fn item(id: u32, kind: u16, extra: Vec<Vec<u8>>) -> Vec<u8> {
    let mut d = vec![0; 64];
    d[..2].copy_from_slice(&kind.to_be_bytes());
    set32(&mut d, 16, id);
    let mut out = vec![chunk(b"idta", &d)];
    out.extend(extra);
    list(b"Item", out)
}
fn comp(id: u32, extra: Vec<Vec<u8>>) -> Vec<u8> {
    let mut d = vec![0; 204];
    set32(&mut d, 8, 8000);
    set32(&mut d, 0x9c, 24 * 65536);
    d[0x8c..0x8e].copy_from_slice(&320u16.to_be_bytes());
    d[0x8e..0x90].copy_from_slice(&240u16.to_be_bytes());
    let mut out = vec![chunk(b"cdta", &d)];
    out.extend(extra);
    item(id, 4, out)
}
fn layer(id: u32, source: u32, kind: u8, parent: u32, props: Vec<Vec<u8>>) -> Vec<u8> {
    let mut d = vec![0; 164];
    set32(&mut d, 0, id);
    set32(&mut d, 0x28, source);
    set32(&mut d, 0x84, parent);
    d[0x83] = kind;
    d[0x27] = 1;
    list(b"Layr", vec![chunk(b"ldta", &d), list(b"tdgp", props)])
}
fn aep(items: Vec<Vec<u8>>) -> Vec<u8> {
    container(b"RIFX", b"Egg!", vec![list(b"Fold", items)])
}
fn project(props: Vec<Vec<u8>>) -> Vec<u8> {
    aep(vec![comp(3, vec![layer(7, 10, 0, 0, props)])])
}
fn property(name: &str, values: &[f64], extra: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    let mut d = vec![0; 124];
    d[3] = values.len() as u8;
    d[0x3b] = 8;
    let data: Vec<u8> = values.iter().flat_map(|v| v.to_be_bytes()).collect();
    let mut out = vec![chunk(b"tdb4", &d), chunk(b"cdat", &data)];
    out.extend(extra);
    vec![chunk(b"tdmn", name.as_bytes()), list(b"tdbs", out)]
}
fn nums(p: &Property) -> Vec<Option<f64>> {
    p.default_value()
        .unwrap()
        .numbers
        .iter()
        .map(|n| n.value)
        .collect()
}

#[test]
fn units_are_sorted_unique_and_unknown_units_stay_absent() {
    let catalog: Vec<_> = known_property_units().collect();
    assert!(catalog.windows(2).all(|w| w[0].0 < w[1].0));
    for (name, unit) in &catalog {
        assert_eq!(property_unit(name), Some(*unit));
    }
    assert_eq!(
        property_unit("ADBE Anchor Point"),
        Some(PropertyUnit::Pixels)
    );
    assert_eq!(property_unit("ADBE Rotate Z"), Some(PropertyUnit::Degrees));
    assert_eq!(property_unit("ADBE Opacity"), Some(PropertyUnit::Percent));
    assert_eq!(
        property_unit("ADBE Audio Levels"),
        Some(PropertyUnit::Decibels)
    );
    assert_eq!(
        property_unit("ADBE Time Remapping"),
        Some(PropertyUnit::Seconds)
    );
    assert_eq!(property_unit("unknown-control"), None);
    let bytes = project(property("unknown-control", &[2.0], vec![]));
    let mut d = parse(&bytes).unwrap();
    let n = &mut d.project.compositions[0].layers[0].properties[0];
    assert_eq!(n.units(), None);
    n.property.as_mut().unwrap().metadata.control_type = Some(3);
    assert_eq!(n.units(), Some(PropertyUnit::Degrees));
    n.match_name = "ADBE Opacity".into(); // Match-name metadata takes precedence over fallback.
    assert_eq!(n.units(), Some(PropertyUnit::Percent));
}
#[test]
fn inferred_defaults_keep_stored_values_and_expose_their_source() {
    let mut props = property("ADBE Position", &[160.0, 120.0, 0.0], vec![]);
    props.extend(property("ADBE Anchor Point", &[7.0, 8.0, 0.0], vec![]));
    props.extend(property("ADBE Scale", &[1.0, 1.0, 1.0], vec![]));
    props.extend(property("ADBE Opacity", &[1.0], vec![]));
    props.extend(property("ADBE Position_1", &[121.0], vec![]));
    let bytes = project(props);
    let d = parse(&bytes).unwrap();
    let l = &d.project.compositions[0].layers[0];
    let p = l.property("ADBE Position").unwrap();
    assert_eq!(
        p.default_value().unwrap().origin,
        DefaultOrigin::CompositionCenter
    );
    assert_eq!(nums(p), [Some(160.0), Some(120.0), Some(0.0)]);
    assert_eq!(p.is_modified(), Some(false));
    let p = l.property("ADBE Anchor Point").unwrap();
    assert_eq!(
        p.default_value().unwrap().origin,
        DefaultOrigin::CompositionSizeFallback
    );
    assert_eq!(
        p.static_value.as_ref().unwrap().numbers().unwrap()[0].value,
        Some(7.0)
    );
    assert_eq!(p.is_modified(), Some(true));
    assert_eq!(
        l.property("ADBE Scale")
            .unwrap()
            .default_value()
            .unwrap()
            .origin,
        DefaultOrigin::FixedConvention
    );
    assert_eq!(nums(l.property("ADBE Position_1").unwrap()), [Some(120.0)]);
    assert_eq!(nums(l.property("ADBE Scale").unwrap()), [Some(1.0); 3]);
    assert_eq!(l.property("ADBE Scale").unwrap().is_modified(), Some(false));
    assert_eq!(nums(l.property("ADBE Opacity").unwrap()), [Some(1.0)]);
    assert_eq!(
        l.property("ADBE Opacity").unwrap().is_modified(),
        Some(false)
    );
}
#[test]
fn null_and_shape_anchor_defaults_use_layer_origin() {
    let props = property("ADBE Anchor Point", &[0.0, 0.0, 0.0], vec![]);
    let bytes = aep(vec![comp(3, vec![layer(7, 0, 0, 0, props)])]);
    let d = parse(&bytes).unwrap();
    let l = &d.project.compositions[0].layers[0];
    assert_eq!(l.kind, LayerKind::Null);
    assert_eq!(
        l.property("ADBE Anchor Point")
            .unwrap()
            .default_value()
            .unwrap()
            .origin,
        DefaultOrigin::LayerOrigin
    );
    let mut props = property("ADBE Anchor Point", &[0.0, 0.0, 0.0], vec![]);
    props.extend([
        chunk(b"tdmn", b"ADBE Root Vectors Group"),
        list(b"tdgp", vec![]),
    ]);
    let bytes = project(props);
    let d = parse(&bytes).unwrap();
    let l = &d.project.compositions[0].layers[0];
    assert_eq!(l.kind, LayerKind::Shape);
    assert_eq!(
        nums(l.property("ADBE Anchor Point").unwrap()),
        [Some(0.0); 3]
    );
}
#[test]
fn file_contained_defaults_win_and_effect_children_are_not_inferred() {
    fn pard(control: u8, body: &[u8]) -> Vec<u8> {
        let mut d = vec![0; 56];
        d[15] = control;
        d.extend(body);
        chunk(b"pard", &d)
    }
    let mut body = vec![0; 64];
    body[..8].copy_from_slice(&25.0f64.to_be_bytes());
    let effect = list(
        b"sspc",
        vec![
            list(
                b"parT",
                vec![
                    chunk(b"tdmn", b"Effect"),
                    pard(13, &[]),
                    chunk(b"tdmn", b"ADBE Opacity"),
                    pard(10, &body),
                ],
            ),
            list(b"tdgp", property("ADBE Opacity", &[25.0], vec![])),
        ],
    );
    let bytes = project(vec![
        chunk(b"tdmn", b"ADBE Effect Parade"),
        list(b"tdgp", vec![chunk(b"tdmn", b"Custom"), effect]),
    ]);
    let mut d = parse(&bytes).unwrap();
    let p = d.project.compositions[0].layers[0].properties[0].children[0].children[0]
        .property
        .as_mut()
        .unwrap();
    assert_eq!(nums(p), [Some(25.0)]);
    assert_eq!(
        p.default_value().unwrap().origin,
        DefaultOrigin::EffectDefinition
    );
    assert!(p.inferred_default.is_none());
    assert_eq!(p.is_modified(), Some(false));
    p.effect_definition = None;
    assert!(p.default_value().is_none());
    assert_eq!(p.is_modified(), None);
}
#[test]
fn modified_state_preserves_unknown_and_non_finite_comparisons() {
    let bytes = project(property("ADBE Opacity", &[1.0], vec![]));
    let mut d = parse(&bytes).unwrap();
    let p = d.project.compositions[0].layers[0].properties[0]
        .property
        .as_mut()
        .unwrap();
    assert_eq!(p.is_modified(), Some(false));
    p.static_value = Some(Value::Numbers(vec![Number::from(f64::NAN)]));
    assert_eq!(p.is_modified(), None);
    p.static_value = Some(Value::Numbers(vec![Number {
        value: Some(f64::INFINITY),
        non_finite: None,
    }]));
    assert_eq!(p.is_modified(), None);
    p.static_value = None;
    assert_eq!(p.is_modified(), None);
    p.expression = Some("0".into());
    p.expression_enabled = Some(false);
    assert_eq!(p.is_modified(), Some(true));
    p.expression = None;
    p.metadata.animated = Some(true);
    assert!(p.is_animated());
    assert_eq!(p.is_modified(), Some(true));
    p.metadata.animated = Some(false);
    assert!(!p.is_animated());
    let bytes = project(property("unknown", &[0.0], vec![]));
    let d = parse(&bytes).unwrap();
    assert_eq!(
        d.project.compositions[0].layers[0].properties[0].is_modified(),
        None
    );
}
#[test]
fn modified_groups_do_not_treat_unknown_children_as_unchanged() {
    let mut props = vec![
        chunk(b"tdmn", b"ADBE Transform Group"),
        list(b"tdgp", property("unknown", &[0.0], vec![])),
    ];
    props.extend([
        chunk(b"tdmn", b"ADBE Root Vectors Group"),
        list(b"tdgp", vec![]),
    ]);
    let bytes = project(props);
    let mut d = parse(&bytes).unwrap();
    let nodes = &mut d.project.compositions[0].layers[0].properties;
    assert_eq!(nodes[0].is_modified(), None);
    assert_eq!(nodes[1].is_modified(), Some(false));
    nodes[0].children[0].property.as_mut().unwrap().expression = Some("1".into());
    assert_eq!(nodes[0].is_modified(), Some(true));
    nodes[0].match_name = "ADBE Root Vectors Group".into();
    assert_eq!(nodes[0].is_modified(), Some(true));
    nodes[0].match_name = "ADBE Vectors Group".into();
    nodes[0].children[0].property.as_mut().unwrap().expression = None;
    assert_eq!(nodes[0].is_modified(), None); // Only the root shape parade is an indexed group.
}
#[test]
fn explicit_names_obey_embedded_lengths_and_retain_provenance() {
    fn named(data: Vec<u8>) -> Vec<u8> {
        let mut props = property("ADBE Opacity", &[100.0], vec![]);
        props[1] = list(b"tdbs", vec![chunk(b"tdsn", &data)]);
        project(props)
    }
    let mut name = b"Utf8".to_vec();
    name.extend(3u32.to_be_bytes());
    name.extend(b"ABCignored");
    let bytes = named(name);
    let d = parse(&bytes).unwrap();
    let n = &d.project.compositions[0].layers[0].properties[0];
    assert_eq!(n.name, "ABC");
    assert_eq!(n.name_source, NameSource::InstanceName);
    assert!(n.has_explicit_name());
    assert!(n.is_name_set());
    let mut name = b"Utf8".to_vec();
    name.extend(u32::MAX.to_be_bytes());
    name.extend(b"short");
    let bytes = named(name);
    let d = parse(&bytes).unwrap();
    let n = &d.project.compositions[0].layers[0].properties[0];
    assert_eq!(n.name, "ADBE Opacity");
    assert_eq!(n.name_source, NameSource::MatchName);
    assert!(
        d.project
            .diagnostics
            .iter()
            .any(|d| d.code == "invalid-property-name")
    );
    let bytes = named(chunk(b"Utf8", b"ADBE Opacity"));
    let d = parse(&bytes).unwrap();
    let n = &d.project.compositions[0].layers[0].properties[0];
    assert!(n.has_explicit_name());
    assert!(!n.is_name_set());
    let bytes = named(chunk(b"Utf8", b""));
    let d = parse(&bytes).unwrap();
    let n = &d.project.compositions[0].layers[0].properties[0];
    assert_eq!(n.name, "");
    assert!(n.has_explicit_name());
    assert!(!n.is_name_set());
    let bytes = named(chunk(b"Utf8", b"-_0_/-"));
    let d = parse(&bytes).unwrap();
    let n = &d.project.compositions[0].layers[0].properties[0];
    assert_eq!(n.name, "ADBE Opacity");
    assert_eq!(n.name_source, NameSource::MatchName);
    assert!(!n.has_explicit_name());
    assert!(!n.is_name_set());
}
#[test]
fn references_remain_local_and_do_not_follow_cycles_or_sentinel_ids() {
    let bytes = aep(vec![
        comp(
            3,
            vec![
                layer(7, 10, 0, 8, vec![]),
                layer(8, 0, 0, 7, vec![]),
                layer(9, 7, 1, 0, vec![]),
                layer(11, 3, 2, 0, vec![]),
            ],
        ),
        comp(4, vec![layer(7, 0, 0, 8, vec![])]),
        item(
            10,
            7,
            vec![list(b"Pin ", vec![chunk(b"Cpth", b"never-open.mov")])],
        ),
    ]);
    let mut d = parse(&bytes).unwrap();
    let p = &d.project;
    let c = &p.compositions[0];
    assert_eq!(c.parent_of(&c.layers[0]).unwrap().id, 8);
    assert_eq!(c.parent_of(&c.layers[1]).unwrap().id, 7);
    assert!(c.parent_of(&p.compositions[1].layers[0]).is_none());
    assert_eq!(p.source_of(&c.layers[0]).unwrap().id(), 10);
    assert!(p.source_of(&c.layers[2]).is_none());
    assert!(p.source_of(&c.layers[3]).is_none());
    assert_eq!(c.light_source_of(&c.layers[2]).unwrap().id, 7);
    assert_eq!(c.active_camera().unwrap().id, 11);
    assert!(p.layer_by_id(0).is_none());
    assert!(p.av_item_by_id(u32::MAX).is_none());
    assert_eq!(p.layers().count(), 5);
    assert_eq!(p.item_children(0).count(), 3);
    assert!(p.item_by_id(10).is_some());
    assert!(p.composition_by_id(99).is_none());
    d.project.compositions[0].layers[0].parent_id = Some(u32::MAX);
    let c = &d.project.compositions[0];
    assert!(c.parent_of(&c.layers[0]).is_none());
}
#[test]
fn legacy_matte_fallback_requires_absent_explicit_field() {
    let bytes = aep(vec![comp(
        3,
        vec![layer(7, 0, 0, 0, vec![]), layer(8, 0, 0, 0, vec![])],
    )]);
    let mut d = parse(&bytes).unwrap();
    let c = &mut d.project.compositions[0];
    c.layers[1].track_matte_mode = Some(1);
    assert!(c.legacy_matte_source_of(&c.layers[1]).is_none()); // Saved explicit zero.
    c.layers[1].track_matte_layer_id = None;
    assert_eq!(c.legacy_matte_source_of(&c.layers[1]).unwrap().id, 7);
    c.layers[1].track_matte_layer_id = Some(7);
    assert_eq!(c.explicit_matte_source_of(&c.layers[1]).unwrap().id, 7);
    assert!(c.legacy_matte_source_of(&c.layers[1]).is_none());
    c.layers[1].track_matte_layer_id = None;
    c.layers[1].track_matte_mode = Some(200);
    assert!(c.legacy_matte_source_of(&c.layers[1]).is_none());
}
#[test]
fn footage_views_preserve_missing_evidence_and_borrow_paths() {
    let bytes = aep(vec![item(
        10,
        7,
        vec![list(b"Pin ", vec![chunk(b"Cpth", b"not-opened.wav")])],
    )]);
    let mut d = parse(&bytes).unwrap();
    let f = &mut d.project.footage[0];
    assert_eq!(f.has_audio(), None);
    assert_eq!(f.file_missing_at_save(), None);
    match f.main_source() {
        FootageSourceRef::File {
            path,
            sequence_members,
        } => {
            assert_eq!(path, Some("not-opened.wav"));
            assert!(sequence_members.is_empty());
        }
        _ => panic!("file view"),
    }
    f.audio_sample_rate = Some(44100.0.into());
    f.missing_at_save = Some(true);
    assert_eq!(f.has_audio(), Some(true));
    assert_eq!(f.file_missing_at_save(), Some(true));
    f.audio_sample_rate = Some(f64::NAN.into());
    assert_eq!(f.has_audio(), None);
    f.is_solid = true;
    assert!(matches!(
        f.main_source(),
        FootageSourceRef::Solid { color: None }
    ));
    assert_eq!(f.has_audio(), Some(false));
    assert_eq!(f.file_missing_at_save(), Some(false));
    f.is_placeholder = true;
    assert!(matches!(f.main_source(), FootageSourceRef::Placeholder));
}
