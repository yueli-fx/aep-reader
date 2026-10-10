//! Narrow, source-checked static tracking animator mapping.
use crate::{Report, Result, map_property};
use aep_reader::{DecodeStatus, PropertyNode};
use effectcraft_project::{PropGroup, build};
use std::collections::BTreeSet;

fn child<'a>(node: &'a PropertyNode, name: &str) -> Result<&'a PropertyNode> {
    let mut found = node.children.iter().filter(|n| n.match_name == name);
    let item = found.next().ok_or_else(|| format!("missing {name}"))?;
    if found.next().is_some() {
        return Err(format!("duplicate {name}"));
    }
    Ok(item)
}
fn static_scalar(node: &PropertyNode) -> Result<f64> {
    let p = node.property.as_ref().ok_or("missing scalar")?;
    if node.decode_status != DecodeStatus::Decoded
        || !p.keyframes.is_empty()
        || p.expression.is_some()
    {
        return Err("only complete static tracking values are supported".into());
    }
    match p.static_value.as_ref().and_then(|v| v.numbers()) {
        Some([n]) => crate::finite(n.value, "tracking value"),
        _ => Err("missing scalar value".into()),
    }
}
fn known_children(node: &PropertyNode, names: &[&str]) -> Result<()> {
    if node.decode_status != DecodeStatus::Decoded
        || node.property.is_some()
        || node
            .children
            .iter()
            .any(|n| !names.contains(&n.match_name.as_str()))
    {
        return Err(format!("unsupported or incomplete {}", node.match_name));
    }
    Ok(())
}
pub(crate) fn convert(
    root: &PropertyNode,
    ids: &mut build::Ids<'_>,
    ticks: u32,
    context: &str,
    mapped: &mut BTreeSet<usize>,
    report: &mut Report,
) -> Result<Vec<PropGroup>> {
    let mut result = Vec::new();
    for node in &root.children {
        let prepare = || -> Result<_> {
            if node.match_name != "ADBE Text Animator" {
                return Err("unknown animator group".into());
            }
            known_children(
                node,
                &["ADBE Text Selectors", "ADBE Text Animator Properties"],
            )?;
            let selectors = child(node, "ADBE Text Selectors")?;
            known_children(selectors, &["ADBE Text Selector"])?;
            let selector = child(selectors, "ADBE Text Selector")?;
            known_children(
                selector,
                &[
                    "ADBE Text Percent Start",
                    "ADBE Text Percent End",
                    "ADBE Text Percent Offset",
                    "ADBE Text Range Advanced",
                ],
            )?;
            let advanced = child(selector, "ADBE Text Range Advanced")?;
            known_children(advanced, &[])?;
            let props = child(node, "ADBE Text Animator Properties")?;
            known_children(
                props,
                &["ADBE Text Track Type", "ADBE Text Tracking Amount"],
            )?;
            let kind = child(props, "ADBE Text Track Type")?;
            if static_scalar(kind)? != 1.0 {
                return Err("unverified tracking type (only Before & After is mapped)".into());
            }
            let leaves = [
                child(selector, "ADBE Text Percent Start")?,
                child(selector, "ADBE Text Percent End")?,
                child(selector, "ADBE Text Percent Offset")?,
                child(props, "ADBE Text Tracking Amount")?,
            ];
            for leaf in leaves {
                static_scalar(leaf)?;
            }
            let enabled = [root, node, selectors, selector, advanced, props, kind]
                .into_iter()
                .chain(leaves)
                .try_fold(true, |value, n| {
                    n.enabled
                        .map(|e| value && e)
                        .ok_or("unknown tracking enable state")
                })?;
            Ok((leaves, kind, enabled))
        };
        let (leaves, kind, enabled) = match prepare() {
            Ok(v) => v,
            Err(e) => {
                report.warn(
                    context,
                    format!("Text animator at byte {} omitted: {e}.", node.source.offset),
                );
                continue;
            }
        };
        let props = build::text_anim_props(ids, "tracking", false);
        let mut group = build::text_animator(ids, &node.name, props);
        group.enabled = enabled;
        for (leaf, path) in leaves.into_iter().zip([
            "selectors/rangeSelector/start",
            "selectors/rangeSelector/end",
            "selectors/rangeSelector/offset",
            "properties/tracking",
        ]) {
            let target = group
                .prop_mut(path)
                .ok_or("missing target tracking property")?;
            map_property(
                leaf.property.as_ref().ok_or("missing tracking scalar")?,
                target,
                &leaf.match_name,
                ticks,
                context,
                report,
            )?;
            mapped.insert(leaf.source.offset);
        }
        mapped.insert(kind.source.offset);
        report.properties += 1;
        report.warn(context,"Static tracking animator imported. Elided range advanced settings use target defaults (percent/characters/add/square, amount/smoothness 100); AE pixel equivalence remains unverified.");
        result.push(group);
    }
    Ok(result)
}
