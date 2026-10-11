//! Static polystars. Reject geometry the target would silently round or over-allocate.
use std::collections::BTreeSet;

use crate::shape_fields::Fields;
use aep_reader::PropertyNode;
use effectcraft_keyframe::Value;
use effectcraft_project::{PropGroup, build};

use crate::{Report, Result};

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
    let star = match fields.scalar("ADBE Vector Star Type", Some(1.0))? {
        1.0 => true,
        2.0 => false,
        _ => return Err("unknown polystar type".into()),
    };
    let direction = match fields.scalar("ADBE Vector Shape Direction", Some(1.0))? {
        1.0 => 0,
        3.0 => 1,
        _ => return Err("unknown path direction".into()),
    };
    let points = fields.scalar("ADBE Vector Star Points", Some(5.0))?;
    if points.fract() != 0.0 || !(3.0..=1024.0).contains(&points) {
        return Err("only static integer point counts in 3..=1024 are mapped; fractional/animated counts need different target geometry".into());
    }
    let position = fields.numbers("ADBE Vector Star Position", Some([0.0; 2]))?;
    let rotation = fields.scalar("ADBE Vector Star Rotation", Some(0.0))?;
    let outer = fields.scalar("ADBE Vector Star Outer Radius", None)?;
    let outer_round = fields.scalar("ADBE Vector Star Outer Roundess", Some(0.0))?;
    let (inner, inner_round) = if star {
        (
            fields.scalar("ADBE Vector Star Inner Radius", None)?,
            fields.scalar("ADBE Vector Star Inner Roundess", Some(0.0))?,
        )
    } else {
        (0.0, 0.0)
    };
    if inner < 0.0 || outer < 0.0 {
        return Err("negative polystar radius".into());
    }
    let mut group = build::shape_star(ids, star, points, position, outer, inner);
    for (name, value) in [
        ("direction", Value::Enum(direction)),
        ("rotation", Value::Scalar(rotation)),
        ("outerRoundness", Value::Scalar(outer_round)),
    ] {
        group
            .prop_mut(name)
            .ok_or("missing target polystar property")?
            .value = value;
    }
    if star {
        group
            .prop_mut("innerRoundness")
            .ok_or("missing target inner roundness")?
            .value = Value::Scalar(inner_round);
    }
    if !fields.defaults.is_empty() {
        report.warn(
            context,
            format!(
                "Polystar omitted fields use explicit importer conventions: {}.",
                fields.defaults.join(", ")
            ),
        );
    }
    if inner_round != 0.0 || outer_round != 0.0 {
        report.warn(context, "Polystar roundness uses the target's curve approximation; AE contour equivalence is unverified.");
    }
    report.properties += fields.offsets.len();
    mapped.extend(fields.offsets);
    Ok(group)
}
