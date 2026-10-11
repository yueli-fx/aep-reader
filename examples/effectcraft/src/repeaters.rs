//! Bounded static repeater geometry. Nested expansion is screened by shape traversal.
use crate::{Report, Result, shape_fields::Fields};
use aep_reader::PropertyNode;
use effectcraft_keyframe::Value;
use effectcraft_project::{PropGroup, build};
use std::collections::BTreeSet;

pub(crate) fn count(nodes: &[PropertyNode]) -> usize {
    nodes
        .iter()
        .map(|n| {
            usize::from(n.match_name == "ADBE Vector Filter - Repeater")
                .saturating_add(count(&n.children))
        })
        .fold(0, usize::saturating_add)
}

pub(crate) fn convert(
    node: &PropertyNode,
    ids: &mut build::Ids<'_>,
    mapped: &mut BTreeSet<usize>,
    context: &str,
    report: &mut Report,
) -> Result<PropGroup> {
    let mut fields = Fields {
        node,
        offsets: Vec::new(),
        defaults: Vec::new(),
    };
    let copies = fields.scalar("ADBE Vector Repeater Copies", None)?;
    if copies.fract() != 0.0 || !(0.0..=128.0).contains(&copies) {
        return Err("only static integer repeater copies in 0..128 are mapped".into());
    }
    let offset = fields.scalar("ADBE Vector Repeater Offset", Some(0.0))?;
    if offset.abs() > 128.0 {
        return Err("repeater offset exceeds expansion budget".into());
    }
    let order = match fields.scalar("ADBE Vector Repeater Order", Some(1.0))? {
        1.0 => 0,
        2.0 => 1,
        _ => return Err("unknown repeater order".into()),
    };
    let mut groups = node
        .children
        .iter()
        .filter(|n| n.match_name == "ADBE Vector Repeater Transform");
    let transform = groups.next().ok_or("missing repeater transform")?;
    if groups.next().is_some() {
        return Err("duplicate repeater transform".into());
    }
    let mut tr = Fields {
        node: transform,
        offsets: Vec::new(),
        defaults: Vec::new(),
    };
    let anchor = tr.numbers("ADBE Vector Repeater Anchor", Some([0.0; 2]))?;
    let position = tr.numbers("ADBE Vector Repeater Position", Some([100.0, 0.0]))?;
    let scale = tr.numbers("ADBE Vector Repeater Scale", Some([100.0; 2]))?;
    let rotation = tr.scalar("ADBE Vector Repeater Rotation", Some(0.0))?;
    let start = tr.scalar("ADBE Vector Repeater Opacity 1", Some(100.0))?;
    let end = tr.scalar("ADBE Vector Repeater Opacity 2", Some(100.0))?;
    if !(0.0..=100.0).contains(&start) || !(0.0..=100.0).contains(&end) {
        return Err("repeater opacity outside 0..100".into());
    }
    for factor in scale {
        if factor <= 0.0 {
            return Err("non-positive repeater scale is not mapped".into());
        }
        for power in [offset, offset + copies] {
            let value = (factor / 100.0).powf(power);
            if !value.is_finite() || !(1e-6..=1e6).contains(&value) {
                return Err("repeater cumulative scale exceeds budget".into());
            }
        }
    }
    let mut group = build::shape_repeater(ids, copies, position);
    group
        .prop_mut("offset")
        .ok_or("missing repeater offset")?
        .value = Value::Scalar(offset);
    group
        .prop_mut("composite")
        .ok_or("missing repeater order")?
        .value = Value::Enum(order);
    for (name, value) in [
        ("transform/anchor", Value::Vec2(anchor)),
        ("transform/scale", Value::Vec2(scale)),
        ("transform/rotation", Value::Scalar(rotation)),
        ("transform/startOpacity", Value::Scalar(start)),
        ("transform/endOpacity", Value::Scalar(end)),
    ] {
        group
            .prop_mut(name)
            .ok_or("missing repeater target property")?
            .value = value;
    }
    if transform.enabled == Some(false) {
        return Err("disabled repeater transform semantics unverified".into());
    }
    fields.defaults.extend(tr.defaults);
    if !fields.defaults.is_empty() {
        report.warn(
            context,
            format!(
                "Repeater omitted fields use importer conventions: {}.",
                fields.defaults.join(", ")
            ),
        );
    }
    report.warn(context,"Static repeater geometry imported; AE copy compositing and transform equivalence require visual comparison.");
    fields.offsets.extend(tr.offsets);
    report.properties += fields.offsets.len();
    mapped.extend(fields.offsets);
    Ok(group)
}
