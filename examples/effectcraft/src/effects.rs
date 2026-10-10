//! Deliberately narrow Gaussian mapping. Definition-derived values are reported.
use crate::{Report, Result, map_property};
use aep_reader as ae;
use effectcraft_keyframe::Value;
use effectcraft_project::{GroupKind, ParamUi, PropGroup, build};
use std::collections::BTreeSet;
const GAUSSIAN: &str = "ADBE Gaussian Blur 2";

fn one<'a, T>(items: impl Iterator<Item = &'a T>) -> Result<Option<&'a T>>
where
    T: 'a,
{
    let mut items = items;
    let first = items.next();
    if items.next().is_some() {
        return Err("ambiguous duplicate parameter".into());
    }
    Ok(first)
}
fn number(value: &Option<ae::Value>) -> Result<f64> {
    let values = value
        .as_ref()
        .and_then(ae::Value::numbers)
        .ok_or("missing numeric definition")?;
    if values.len() != 1 {
        return Err("expected scalar definition".into());
    }
    crate::finite(values.first().and_then(|v| v.value), "effect value")
}
fn definition_value(def: &ae::EffectParameterDefinition, suffix: &str) -> Result<f64> {
    if def.decode_status != ae::DecodeStatus::Decoded {
        return Err("partial parameter definition".into());
    }
    let last = number(&def.last_value)?;
    let default = number(&def.default_value)?;
    match suffix {
        "0001" if def.control_type == 10 && last == 0.0 && default == 0.0 => Ok(0.0),
        // Only the observed untouched Gaussian layout; not a general enum decoder.
        "0002"
            if def.control_type == 7
                && def.option_count == Some(3)
                && last == 1.0
                && default == 0.0 =>
        {
            Ok(1.0)
        }
        "0003" if def.control_type == 4 && last == default && matches!(default, 0.0 | 1.0) => {
            Ok(default)
        }
        _ => Err("unverified Gaussian definition layout/value".into()),
    }
}
fn static_value(
    project: &ae::Project,
    effect: &ae::PropertyNode,
    suffix: &str,
) -> Result<(f64, String)> {
    let name = format!("{GAUSSIAN}-{suffix}");
    if let Some(node) = one(effect.children.iter().filter(|n| n.match_name == name))? {
        let p = node.property.as_ref().ok_or("missing parameter payload")?;
        if p.is_animated() || p.expression.as_ref().is_some_and(|s| !s.is_empty()) {
            return Err("animated/expression selector is not mapped".into());
        }
        return Ok((
            number(&p.static_value)?,
            format!("explicit value at byte {}", node.source.offset),
        ));
    }
    let global = project.effect_definition(GAUSSIAN);
    let (table, scope) = if effect.effect_parameters.is_empty() {
        (
            &global
                .ok_or("missing or ambiguous project definition")?
                .parameters,
            "project",
        )
    } else {
        (&effect.effect_parameters, "instance")
    };
    let def = one(table.iter().filter(|d| d.match_name == name))?
        .ok_or("missing parameter definition")?;
    let value = definition_value(def, suffix)?;
    if scope == "instance" {
        let other = global
            .map(|g| one(g.parameters.iter().filter(|d| d.match_name == name)))
            .transpose()?
            .flatten();
        if let Some(other) = other
            && definition_value(other, suffix)? != value
        {
            return Err("instance/project definition conflict".into());
        }
    }
    Ok((
        value,
        format!(
            "definition-derived ({scope}, byte {}); effective AE value requires comparison",
            def.source.offset
        ),
    ))
}

pub(crate) fn convert(
    project: &ae::Project,
    nodes: &[ae::PropertyNode],
    ids: &mut build::Ids<'_>,
    ticks: u32,
    context: &str,
    mapped: &mut BTreeSet<usize>,
    report: &mut Report,
) -> Result<Vec<PropGroup>> {
    let mut groups = Vec::new();
    for node in nodes {
        if node.match_name != GAUSSIAN {
            continue;
        }
        let ctx = format!("{context}/{GAUSSIAN}@{}", node.source.offset);
        let blur_name = format!("{GAUSSIAN}-0001");
        let prepare = || -> Result<_> {
            let built_in = one(node
                .children
                .iter()
                .filter(|n| n.match_name == "ADBE Effect Built In Params"))?;
            if built_in.is_some_and(|n| {
                !n.children.is_empty()
                    || n.property.is_some()
                    || n.decode_status != ae::DecodeStatus::Decoded
            }) {
                return Err(
                    "non-default or incomplete effect compositing options are not supported".into(),
                );
            }
            let blur = one(node.children.iter().filter(|n| n.match_name == blur_name))?;
            let fallback = if blur.is_none() {
                Some(static_value(project, node, "0001")?)
            } else {
                None
            };
            let (dims, dims_origin) = static_value(project, node, "0002")?;
            let (repeat, repeat_origin) = static_value(project, node, "0003")?;
            let dimension = match dims {
                1.0 => 0,
                2.0 => 1,
                3.0 => 2,
                _ => return Err("invalid Gaussian dimensions".into()),
            };
            let repeat = match repeat {
                0.0 => false,
                1.0 => true,
                _ => return Err("invalid Gaussian repeat-edge value".into()),
            };
            if let Some(blur) = blur {
                let property = blur.property.as_ref().ok_or("missing blurriness payload")?;
                if property.static_value.is_none() && property.keyframes.is_empty() {
                    return Err("blurriness record has no saved value or keys".into());
                }
            }
            Ok((
                blur,
                fallback,
                dimension,
                dims_origin,
                repeat,
                repeat_origin,
            ))
        };
        let (blur, fallback, dimension, dims_origin, repeat, repeat_origin) = match prepare() {
            Ok(v) => v,
            Err(reason) => {
                report.warn(&ctx, format!("Gaussian omitted: {reason}."));
                continue;
            }
        };
        let mut group = ids.group("ec.blur.gaussian", &node.name);
        group.kind = GroupKind::Effect {
            effect: "ec.blur.gaussian".into(),
        };
        group.enabled = node.enabled.unwrap_or(false);
        if node.enabled.is_none() {
            report.warn(&ctx, "Effect enabled state unknown; imported disabled.");
        }
        let mut amount = ids.prop("blurriness", "Blurriness", Value::Scalar(0.0));
        amount.ui = ParamUi::Slider {
            min: 0.0,
            max: 3000.0,
            slider_min: 0.0,
            slider_max: 50.0,
            decimals: 1,
        };
        if let Some(source) = blur.and_then(|n| n.property.as_ref()) {
            map_property(source, &mut amount, &blur_name, ticks, &ctx, report)?;
        } else if let Some((value, origin)) = fallback {
            amount.value = Value::Scalar(value);
            report.warn(&ctx, format!("Blurriness: {origin}."));
        }
        let mut dims = ids.prop("dimensions", "Blur Dimensions", Value::Enum(dimension));
        dims.ui = ParamUi::Popup {
            options: vec![
                "Horizontal and Vertical".into(),
                "Horizontal".into(),
                "Vertical".into(),
            ],
        };
        dims.hold_only = true;
        let mut edge = ids.prop("repeatEdge", "Repeat Edge Pixels", Value::Bool(repeat));
        edge.ui = ParamUi::Checkbox;
        edge.hold_only = true;
        group.children = vec![amount.into(), dims.into(), edge.into()];
        for suffix in ["0001", "0002", "0003"] {
            if let Some(source) = node
                .children
                .iter()
                .find(|n| n.match_name == format!("{GAUSSIAN}-{suffix}"))
            {
                mapped.insert(source.source.offset);
            }
        }
        report.warn(&ctx,format!("Gaussian dimensions: {dims_origin}; repeat-edge: {repeat_origin}. Target kernel is approximate; effect compositing options are not mapped."));
        groups.push(group);
    }
    Ok(groups)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    fn definition(control: u8, last: f64, default: f64) -> ae::EffectParameterDefinition {
        ae::EffectParameterDefinition {
            match_name: String::new(),
            name: String::new(),
            control_type: control,
            last_value: Some(ae::Value::Numbers(vec![last.into()])),
            default_value: Some(ae::Value::Numbers(vec![default.into()])),
            min_value: None,
            max_value: None,
            option_count: Some(3),
            enum_options: Vec::new(),
            decode_status: ae::DecodeStatus::Decoded,
            source: ae::RawSpan {
                offset: 0,
                length: 0,
            },
        }
    }
    #[test]
    fn only_observed_gaussian_definition_layouts_are_accepted() {
        assert_eq!(
            definition_value(&definition(7, 1.0, 0.0), "0002").unwrap(),
            1.0
        );
        assert!(definition_value(&definition(7, 2.0, 0.0), "0002").is_err());
        assert!(definition_value(&definition(7, 1.0, 1.0), "0002").is_err());
        assert_eq!(
            definition_value(&definition(4, 0.0, 0.0), "0003").unwrap(),
            0.0
        );
        assert_eq!(
            definition_value(&definition(4, 1.0, 1.0), "0003").unwrap(),
            1.0
        );
        assert!(definition_value(&definition(4, 0.0, 1.0), "0003").is_err());
        assert!(definition_value(&definition(4, 2.0, 2.0), "0003").is_err());
        assert!(definition_value(&definition(10, f64::NAN, 0.0), "0001").is_err());
        let mut partial = definition(10, 0.0, 0.0);
        partial.decode_status = ae::DecodeStatus::Partial;
        assert!(definition_value(&partial, "0001").is_err());
    }
    #[test]
    fn ambiguous_parameter_does_not_pick_the_first() {
        assert!(one([1, 2].iter()).is_err());
    }
    #[test]
    fn unsupported_compositing_does_not_apply_a_full_strength_effect() {
        fn node(name: &str, children: Vec<ae::PropertyNode>) -> ae::PropertyNode {
            ae::PropertyNode {
                enabled: Some(true),
                match_name: name.into(),
                name: name.into(),
                name_source: ae::NameSource::MatchName,
                kind: ae::NodeKind::Group,
                decode_status: ae::DecodeStatus::Decoded,
                children,
                property: None,
                mask: None,
                index: 0,
                depth: 0,
                effect_parameters: Vec::new(),
                source: ae::RawSpan {
                    offset: 123,
                    length: 0,
                },
            }
        }
        let effect = node(
            GAUSSIAN,
            vec![node(
                "ADBE Effect Built In Params",
                vec![node("Unsupported compositing option", vec![])],
            )],
        );
        let mut next_id = 1;
        let mut report = Report::default();
        let mut mapped = BTreeSet::new();
        let result = convert(
            &ae::Project::default(),
            &[effect],
            &mut build::Ids(&mut next_id),
            24000,
            "test",
            &mut mapped,
            &mut report,
        )
        .unwrap();
        assert!(result.is_empty());
        assert!(mapped.is_empty());
        assert_eq!(next_id, 1);
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.message.contains("effect compositing options"))
        );
    }
}
