//! Text animator channels and ordered range selectors. Expressions are never run.
use crate::{Report, Result, map_property, shape_fields::Fields};
use aep_reader::{DecodeStatus, PropertyNode};
use effectcraft_keyframe::Value;
use effectcraft_project::{PropGroup, build};
use std::collections::BTreeSet;

fn optional<'a>(node: &'a PropertyNode, name: &str) -> Result<Option<&'a PropertyNode>> {
    let mut found = node.children.iter().filter(|n| n.match_name == name);
    let item = found.next();
    if found.next().is_some() {
        return Err(format!("duplicate {name}"));
    }
    Ok(item)
}
fn child<'a>(node: &'a PropertyNode, name: &str) -> Result<&'a PropertyNode> {
    optional(node, name)?.ok_or_else(|| format!("missing {name}"))
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
fn enabled(node: &PropertyNode) -> Result<bool> {
    node.enabled
        .ok_or_else(|| format!("unknown enable state for {}", node.match_name))
}
fn channel(
    node: &PropertyNode,
    target: &mut effectcraft_project::Property,
    ticks: u32,
    context: &str,
    mapped: &mut BTreeSet<usize>,
    report: &mut Report,
) -> Result<()> {
    let p = node.property.as_ref().ok_or("missing animator property")?;
    if node.decode_status != DecodeStatus::Decoded
        || p.expression.as_ref().is_some_and(|e| !e.is_empty())
        || (p.static_value.is_none() && p.keyframes.is_empty())
        || p.keyframes.len() > 4096
    {
        return Err("animator channel is incomplete, excessive or expression-driven".into());
    }
    for value in p
        .static_value
        .iter()
        .chain(p.keyframes.iter().filter_map(|k| k.value.as_ref()))
    {
        let numbers = value.numbers().ok_or("animator requires numeric values")?;
        for n in numbers {
            if crate::finite(n.value, "animator value")?.abs() > 1_000_000.0 {
                return Err("animator value exceeds importer budget".into());
            }
        }
        if matches!(
            node.match_name.as_str(),
            "ADBE Text Position 3D" | "ADBE Text Scale 3D"
        ) {
            let expected = if node.match_name == "ADBE Text Scale 3D" {
                100.0
            } else {
                0.0
            };
            if numbers.len() != 3 || numbers.get(2).and_then(|n| n.value) != Some(expected) {
                return Err("per-character Z transform requires separate 3D mapping".into());
            }
        }
    }
    if p.keyframes.iter().any(|k| k.value.is_none()) {
        return Err("undecoded animator key value".into());
    }
    map_property(p, target, &node.match_name, ticks, context, report)?;
    mapped.insert(node.source.offset);
    Ok(())
}

const CHANNELS: &[(&str, &str, &str)] = &[
    ("ADBE Text Opacity", "opacity", "opacity"),
    ("ADBE Text Position 3D", "position", "position"),
    ("ADBE Text Scale 3D", "scale", "scale"),
    ("ADBE Text Rotation", "rotation", "rotation"),
    ("ADBE Text Skew", "skew", "skew"),
    ("ADBE Text Skew Axis", "skew", "skewAxis"),
    ("ADBE Text Fill Color", "fillColor", "fillColor"),
    ("ADBE Text Fill Opacity", "fillOpacity", "fillOpacity"),
    ("ADBE Text Stroke Color", "strokeColor", "strokeColor"),
    ("ADBE Text Stroke Opacity", "strokeOpacity", "strokeOpacity"),
    ("ADBE Text Stroke Width", "strokeWidth", "strokeWidth"),
    ("ADBE Text Tracking Amount", "tracking", "tracking"),
    (
        "ADBE Text Character Offset",
        "characterOffset",
        "characterOffset",
    ),
];
const ADVANCED: &[(&str, &str)] = &[
    ("ADBE Text Selector Max Amount", "amount"),
    ("ADBE Text Selector Smoothness", "smoothness"),
    ("ADBE Text Levels Max Ease", "easeHigh"),
    ("ADBE Text Levels Min Ease", "easeLow"),
    ("ADBE Text Random Seed", "randomSeed"),
];
const POPUPS: &[(&str, &str, u32)] = &[
    ("ADBE Text Range Units", "units", 2),
    ("ADBE Text Range Type2", "basedOn", 4),
    ("ADBE Text Selector Mode", "mode", 6),
    ("ADBE Text Range Shape", "shape", 6),
];

fn selector(
    node: &PropertyNode,
    ids: &mut build::Ids<'_>,
    ticks: u32,
    mapped: &mut BTreeSet<usize>,
    report: &mut Report,
    context: &str,
) -> Result<PropGroup> {
    known_children(
        node,
        &[
            "ADBE Text Percent Start",
            "ADBE Text Percent End",
            "ADBE Text Percent Offset",
            "ADBE Text Index Start",
            "ADBE Text Index End",
            "ADBE Text Index Offset",
            "ADBE Text Range Advanced",
        ],
    )?;
    let mut out = build::range_selector(ids, &node.name);
    out.enabled = enabled(node)?;
    let mut units = 0;
    if let Some(advanced) = optional(node, "ADBE Text Range Advanced")? {
        let allowed: Vec<_> = ADVANCED
            .iter()
            .map(|(n, _)| *n)
            .chain(POPUPS.iter().map(|(n, _, _)| *n))
            .chain(["ADBE Text Randomize Order"])
            .collect();
        known_children(advanced, &allowed)?;
        out.enabled &= enabled(advanced)?;
        let mut f = Fields {
            node: advanced,
            offsets: Vec::new(),
            defaults: Vec::new(),
        };
        for &(name, path, max) in POPUPS {
            if let Some(n) = optional(advanced, name)? {
                out.enabled &= enabled(n)?;
            }
            let v = f.scalar(name, Some(1.0))?;
            if v.fract() != 0.0 || !(1.0..=f64::from(max)).contains(&v) {
                return Err(format!("invalid text selector {name}"));
            }
            if path == "units" {
                units = v as u32 - 1;
            }
            out.prop_mut(&format!("advanced/{path}"))
                .ok_or("missing target selector popup")?
                .value = Value::Enum(v as u32 - 1);
        }
        if let Some(n) = optional(advanced, "ADBE Text Randomize Order")? {
            out.enabled &= enabled(n)?;
        }
        let random = f.scalar("ADBE Text Randomize Order", Some(0.0))?;
        if !matches!(random, 0.0 | 1.0) {
            return Err("invalid selector randomize flag".into());
        }
        out.prop_mut("advanced/randomize")
            .ok_or("missing randomize flag")?
            .value = Value::Bool(random == 1.0);
        if random == 1.0 {
            report.warn(context, "Randomized text order uses the target algorithm; AE random-sequence equivalence is unverified.");
        }
        report.properties += f.offsets.len();
        mapped.extend(f.offsets);
        for &(name, path) in ADVANCED {
            if let Some(n) = optional(advanced, name)? {
                out.enabled &= enabled(n)?;
                channel(
                    n,
                    out.prop_mut(&format!("advanced/{path}"))
                        .ok_or("missing target selector field")?,
                    ticks,
                    context,
                    mapped,
                    report,
                )?;
            }
        }
    }
    let names = if units == 0 {
        [
            "ADBE Text Percent Start",
            "ADBE Text Percent End",
            "ADBE Text Percent Offset",
        ]
    } else {
        [
            "ADBE Text Index Start",
            "ADBE Text Index End",
            "ADBE Text Index Offset",
        ]
    };
    for (name, path) in names.into_iter().zip(["start", "end", "offset"]) {
        if let Some(n) = optional(node, name)? {
            out.enabled &= enabled(n)?;
            channel(
                n,
                out.prop_mut(path).ok_or("missing target range field")?,
                ticks,
                context,
                mapped,
                report,
            )?;
        } else if units == 1 && path == "end" {
            return Err(
                "index selector has no saved end; text-length default is unverified".into(),
            );
        }
    }
    report.warn(context, "Elided selector fields use reported target conventions (percent 0..100, offset 0, characters/add/square, amount/smoothness 100, ease/randomize 0). Range shapes and easing require AE visual comparison.");
    Ok(out)
}

fn animator(
    node: &PropertyNode,
    root: &PropertyNode,
    ids: &mut build::Ids<'_>,
    ticks: u32,
    context: &str,
    mapped: &mut BTreeSet<usize>,
    report: &mut Report,
) -> Result<PropGroup> {
    if node.match_name != "ADBE Text Animator" {
        return Err("unknown animator group".into());
    }
    known_children(
        node,
        &["ADBE Text Selectors", "ADBE Text Animator Properties"],
    )?;
    let props = child(node, "ADBE Text Animator Properties")?;
    let allowed: Vec<_> = CHANNELS
        .iter()
        .map(|(n, _, _)| *n)
        .chain([
            "ADBE Text Track Type",
            "ADBE Text Character Change Type",
            "ADBE Text Character Range",
        ])
        .collect();
    known_children(props, &allowed)?;
    let mut on = enabled(root)? && enabled(node)? && enabled(props)?;
    let mut target_props = Vec::new();
    let mut kinds = BTreeSet::new();
    let mut leaves = Vec::new();
    for &(name, kind, path) in CHANNELS {
        if let Some(n) = optional(props, name)? {
            on &= enabled(n)?;
            if kinds.insert(kind) {
                target_props.extend(build::text_anim_props(ids, kind, false));
            }
            leaves.push((n, path));
        }
    }
    if leaves.is_empty() {
        return Err("no supported text animator channels".into());
    }
    let mut f = Fields {
        node: props,
        offsets: Vec::new(),
        defaults: Vec::new(),
    };
    for (name, needed) in [
        ("ADBE Text Track Type", kinds.contains("tracking")),
        (
            "ADBE Text Character Change Type",
            kinds.contains("characterOffset"),
        ),
        (
            "ADBE Text Character Range",
            kinds.contains("characterOffset"),
        ),
    ] {
        if needed {
            if f.scalar(name, Some(1.0))? != 1.0 {
                return Err("unverified animator mode".into());
            }
            if let Some(n) = optional(props, name)? {
                on &= enabled(n)?;
            }
        } else if optional(props, name)?.is_some() {
            return Err("animator companion without its value channel".into());
        }
    }
    mapped.extend(f.offsets.iter().copied());
    report.properties += f.offsets.len();
    if !f.defaults.is_empty() {
        report.warn(
            context,
            format!(
                "Elided animator companions use mode 1 conventions: {}.",
                f.defaults.join(", ")
            ),
        );
    }
    let mut group = build::text_animator(ids, &node.name, target_props);
    group.enabled = on;
    for (n, path) in leaves {
        channel(
            n,
            group
                .prop_mut(&format!("properties/{path}"))
                .ok_or("missing target animator channel")?,
            ticks,
            context,
            mapped,
            report,
        )?;
    }
    let selectors = child(node, "ADBE Text Selectors")?;
    known_children(selectors, &["ADBE Text Selector"])?;
    if selectors.children.is_empty() || selectors.children.len() > 32 {
        return Err("unsupported empty/excessive selector stack".into());
    }
    group.enabled &= enabled(selectors)?;
    let mut converted = Vec::new();
    for n in &selectors.children {
        converted.push(selector(n, ids, ticks, mapped, report, context)?.into());
    }
    group
        .group_mut("selectors")
        .ok_or("missing target selectors")?
        .children = converted;
    if kinds.contains("characterOffset") {
        report.warn(context,"Character offset uses target default character alignment and preserve-case range; source change-type alignment equivalence is unverified.");
    }
    Ok(group)
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
        // Commit consumed offsets and counts only after the entire animator succeeds.
        let mut local = Report::default();
        let mut consumed = BTreeSet::new();
        match animator(node, root, ids, ticks, context, &mut consumed, &mut local) {
            Ok(group) => {
                report.properties += local.properties;
                report.keyframes += local.keyframes;
                report.warnings.extend(local.warnings);
                mapped.extend(consumed);
                result.push(group);
            }
            Err(e) => report.warn(
                context,
                format!("Text animator at byte {} omitted: {e}.", node.source.offset),
            ),
        }
    }
    Ok(result)
}
