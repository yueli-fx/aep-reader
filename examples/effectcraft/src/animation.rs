//! Independent position axes and saved source-time channels.
use crate::{Report, Result, map_property};
use aep_reader::Layer;
use effectcraft_keyframe::Value;
use effectcraft_project::{Layer as Target, LayerSource, build};
use std::collections::BTreeSet;

pub(crate) fn channels(
    source: &Layer,
    target: &mut Target,
    ids: &mut build::Ids<'_>,
    ticks: u32,
    mapped: &mut BTreeSet<usize>,
    context: &str,
    report: &mut Report,
) -> Result<()> {
    let nodes = source
        .properties
        .iter()
        .find(|n| n.match_name == "ADBE Transform Group")
        .map(|n| n.children.as_slice())
        .unwrap_or(&source.properties);
    let leaders: Vec<_> = nodes
        .iter()
        .filter(|n| n.match_name == "ADBE Position")
        .collect();
    if leaders.len() > 1 {
        report.warn(
            context,
            "Duplicate position leader; separated-position channels not applied.",
        );
    } else if leaders
        .first()
        .and_then(|n| n.property.as_ref())
        .is_some_and(|p| p.metadata.dimensions_separated == Some(true))
    {
        let baseline = target
            .props
            .prop("transform/position")
            .map(|p| p.value.as_vec3())
            .unwrap_or([0.0; 3]);
        let mut properties = Vec::new();
        let mut offsets = Vec::new();
        for ((source_name, target_name), default) in [
            ("ADBE Position_0", "positionX"),
            ("ADBE Position_1", "positionY"),
            ("ADBE Position_2", "positionZ"),
        ]
        .into_iter()
        .zip(baseline)
        {
            let mut matches = nodes.iter().filter(|n| n.match_name == source_name);
            let node = matches.next();
            if matches.next().is_some() {
                return Err(format!("duplicate {source_name}"));
            }
            let mut out = ids.prop(target_name, source_name, Value::Scalar(default));
            if let Some(n) = node {
                let p = n
                    .property
                    .as_ref()
                    .ok_or("separated axis missing payload")?;
                map_property(p, &mut out, source_name, ticks, context, report)?;
                offsets.push(n.source.offset);
            } else {
                report.warn(context,format!("Separated {source_name} absent; combined/default position component retained."));
            }
            properties.push(out);
        }
        let transform = target
            .props
            .group_mut("transform")
            .ok_or("missing target transform")?;
        transform
            .children
            .extend(properties.into_iter().map(Into::into));
        mapped.extend(offsets);
        report.warn(context,"Separated position axes keep independent key times; interpolation approximations are reported per channel.");
    }
    // A disabled remap slot is commonly saved. Presence alone is not activation.
    if let Some(node) = source.node("ADBE Time Remapping")
        && let Some(p) = &node.property
        && !p.keyframes.is_empty()
    {
        if !matches!(
            target.source,
            LayerSource::Comp { .. } | LayerSource::Footage { .. }
        ) {
            report.warn(context,"Time-remap keys found on a layer without a mapped time-based source; remap omitted.");
        } else {
            let mut out = ids.prop("timeRemap", "Time Remap", Value::Scalar(0.0));
            map_property(p, &mut out, "ADBE Time Remapping", ticks, context, report)?;
            if out.keys.is_empty() {
                report.warn(
                    context,
                    "No decoded time-remap keys; source clock retained.",
                );
            } else {
                target.props.children.push(out.into());
                mapped.insert(node.source.offset);
                report.warn(context,"Time remap uses saved seconds and key presence for activation; static/expression-only remap enablement remains unverified.");
            }
        }
    }
    Ok(())
}
