//! Ordered shape contents. Geometry uses the reader's validated coordinate view.
use std::collections::BTreeSet;

use aep_reader::{PropertyNode, ShapeKind};
use effectcraft_keyframe::ShapePath;
use effectcraft_project::{PropGroup, build};

use crate::{Report, Result, map_property};

fn enabled(node: &PropertyNode, target: &mut PropGroup, context: &str, report: &mut Report) {
    if let Some(value) = node.enabled {
        target.enabled = value;
    } else {
        report.warn(
            context,
            format!(
                "Enable state unknown at byte {}; target default retained.",
                node.source.offset
            ),
        );
    }
}

pub(crate) fn masks(
    nodes: &[PropertyNode],
    ids: &mut build::Ids<'_>,
    ticks: u32,
    context: &str,
    scale: [f64; 2],
    mapped: &mut BTreeSet<usize>,
    report: &mut Report,
) -> Result<Vec<PropGroup>> {
    let mut result = Vec::new();
    for node in nodes {
        let Some(info) = &node.mask else {
            continue;
        };
        let mode = match info.mode {
            Some(0) => effectcraft_project::MaskMode::None,
            Some(1) => effectcraft_project::MaskMode::Add,
            Some(2) => effectcraft_project::MaskMode::Subtract,
            Some(3) => effectcraft_project::MaskMode::Intersect,
            Some(4) => effectcraft_project::MaskMode::Lighten,
            Some(5) => effectcraft_project::MaskMode::Darken,
            Some(6) => effectcraft_project::MaskMode::Difference,
            _ => {
                report.warn(
                    context,
                    format!(
                        "Mask {} mode {:?} has no verified mapping; omitted.",
                        node.name, info.mode
                    ),
                );
                continue;
            }
        };
        let mut out = build::mask(
            ids,
            &node.name,
            ShapePath::default(),
            mode,
            info.color.unwrap_or([255, 255, 0]),
        );
        enabled(node, &mut out, context, report);
        if let effectcraft_project::GroupKind::Mask {
            inverted, locked, ..
        } = &mut out.kind
        {
            *inverted = info.inverted.unwrap_or(false);
            *locked = info.locked.unwrap_or(false);
        }
        properties(
            node,
            &mut out,
            &[
                ("ADBE Mask Shape", "path"),
                ("ADBE Mask Feather", "feather"),
                ("ADBE Mask Opacity", "opacity"),
                ("ADBE Mask Offset", "expansion"),
            ],
            ticks,
            context,
            mapped,
            report,
        )?;
        if let Some(property) = out.prop_mut("path") {
            for value in std::iter::once(&mut property.value)
                .chain(property.keys.iter_mut().map(|k| &mut k.value))
            {
                if let effectcraft_keyframe::Value::Path(path) = value {
                    for point in path
                        .vertices
                        .iter_mut()
                        .chain(&mut path.in_tangents)
                        .chain(&mut path.out_tangents)
                    {
                        for (component, factor) in point.iter_mut().zip(scale) {
                            *component =
                                crate::finite(Some(*component * factor), "mask coordinate")?;
                        }
                    }
                }
            }
        }
        if info.motion_blur.is_some_and(|v| v != 0) || info.feather_falloff.is_some_and(|v| v != 0)
        {
            report.warn(
                context,
                "Mask motion blur/falloff settings require further mapping.",
            );
        }
        result.push(out);
    }
    Ok(result)
}

pub(crate) fn contents(
    nodes: &[PropertyNode],
    ids: &mut build::Ids<'_>,
    ticks: u32,
    context: &str,
    mapped: &mut BTreeSet<usize>,
    report: &mut Report,
) -> Result<Vec<PropGroup>> {
    let operators_allowed = crate::operators::within_budget(nodes);
    contents_bounded(
        nodes,
        ids,
        ticks,
        context,
        mapped,
        report,
        operators_allowed,
    )
}

fn contents_bounded(
    nodes: &[PropertyNode],
    ids: &mut build::Ids<'_>,
    ticks: u32,
    context: &str,
    mapped: &mut BTreeSet<usize>,
    report: &mut Report,
    operators_allowed: bool,
) -> Result<Vec<PropGroup>> {
    let repeater_count = crate::repeaters::count(nodes);
    let mut result = Vec::new();
    for node in nodes {
        let name = if node.name.is_empty() {
            &node.match_name
        } else {
            &node.name
        };
        let path = format!("{context}/{name}[{}]", node.index);
        let (mut group, mapping): (PropGroup, &[(&str, &str)]) = match ShapeKind::from_match_name(
            &node.match_name,
        ) {
            ShapeKind::Group => {
                let children = node
                    .children
                    .iter()
                    .find(|n| n.match_name == "ADBE Vectors Group");
                let inner = match children {
                    Some(n) => contents_bounded(
                        &n.children,
                        ids,
                        ticks,
                        &path,
                        mapped,
                        report,
                        operators_allowed,
                    )?,
                    None => Vec::new(),
                };
                let mut group = build::shape_group(ids, name, inner);
                enabled(node, &mut group, &path, report);
                if children.is_some_and(|n| n.enabled == Some(false)) {
                    group.enabled = false;
                }
                if let Some(transform) = node
                    .children
                    .iter()
                    .find(|n| n.match_name == "ADBE Vector Transform Group")
                    && let Some(target) = group.group_mut("transform")
                {
                    properties(transform, target, TRANSFORM, ticks, &path, mapped, report)?;
                }
                result.push(group);
                continue;
            }
            ShapeKind::Rectangle => (
                build::shape_rect(ids, [100.0; 2], [0.0; 2], 0.0),
                &[
                    ("ADBE Vector Rect Size", "size"),
                    ("ADBE Vector Rect Position", "position"),
                    ("ADBE Vector Rect Roundness", "roundness"),
                ],
            ),
            ShapeKind::Ellipse => (
                build::shape_ellipse(ids, [100.0; 2], [0.0; 2]),
                &[
                    ("ADBE Vector Ellipse Size", "size"),
                    ("ADBE Vector Ellipse Position", "position"),
                ],
            ),
            ShapeKind::Star => {
                match crate::stars::convert(node, ids, mapped, &path, report) {
                    Ok(mut group) => {
                        group.name = name.clone();
                        enabled(node, &mut group, &path, report);
                        result.push(group);
                    }
                    Err(reason) => report.warn(&path, format!("Polystar omitted: {reason}.")),
                }
                continue;
            }
            ShapeKind::GradientFill | ShapeKind::GradientStroke | ShapeKind::Repeater => {
                let kind = ShapeKind::from_match_name(&node.match_name);
                let converted = if kind == ShapeKind::Repeater {
                    if repeater_count > 1 {
                        Err("multiple/nested repeaters in this scope exceed the verified expansion contract".into())
                    } else {
                        crate::repeaters::convert(node, ids, mapped, &path, report)
                    }
                } else {
                    crate::gradients::convert(
                        node,
                        kind == ShapeKind::GradientStroke,
                        ids,
                        ticks,
                        mapped,
                        &path,
                        report,
                    )
                };
                match converted {
                    Ok(mut group) => {
                        group.name = name.clone();
                        enabled(node, &mut group, &path, report);
                        result.push(group);
                    }
                    Err(reason) => {
                        report.warn(&path, format!("{} omitted: {reason}.", node.match_name))
                    }
                }
                continue;
            }
            ShapeKind::RoundCorners
            | ShapeKind::PuckerBloat
            | ShapeKind::Twist
            | ShapeKind::ZigZag
            | ShapeKind::MergePaths
            | ShapeKind::OffsetPaths => {
                let converted = if operators_allowed {
                    crate::operators::convert(node, ids, mapped, ticks, &path, report)
                } else {
                    Err("layer path-modifier expansion exceeds importer budget".into())
                };
                match converted {
                    Ok(mut group) => {
                        group.name = name.clone();
                        enabled(node, &mut group, &path, report);
                        result.push(group);
                    }
                    Err(reason) => {
                        report.warn(&path, format!("{} omitted: {reason}.", node.match_name))
                    }
                }
                continue;
            }
            ShapeKind::Path => (
                build::shape_path(ids, ShapePath::default()),
                &[("ADBE Vector Shape", "path")],
            ),
            ShapeKind::Fill => (
                build::shape_fill(ids, [1.0; 4]),
                &[
                    ("ADBE Vector Fill Color", "color"),
                    ("ADBE Vector Fill Opacity", "opacity"),
                ],
            ),
            ShapeKind::Stroke => (
                build::shape_stroke(ids, [1.0; 4], 2.0),
                &[
                    ("ADBE Vector Stroke Color", "color"),
                    ("ADBE Vector Stroke Opacity", "opacity"),
                    ("ADBE Vector Stroke Width", "width"),
                    ("ADBE Vector Stroke Miter Limit", "miter"),
                ],
            ),
            ShapeKind::TrimPaths => (
                build::shape_trim(ids, 0.0, 100.0, 0.0),
                &[
                    ("ADBE Vector Trim Start", "start"),
                    ("ADBE Vector Trim End", "end"),
                    ("ADBE Vector Trim Offset", "offset"),
                ],
            ),
            _ => {
                report.warn(
                    &path,
                    "Shape operator not mapped; original ordered node is retained only in AEP.",
                );
                continue;
            }
        };
        group.name = name.clone();
        enabled(node, &mut group, &path, report);
        let mut consumed = BTreeSet::new();
        let counts = (report.properties, report.keyframes);
        if let Err(reason) = properties(
            node,
            &mut group,
            mapping,
            ticks,
            &path,
            &mut consumed,
            report,
        ) {
            report.properties = counts.0;
            report.keyframes = counts.1;
            report.warn(
                &path,
                format!("Shape omitted after invalid property data: {reason}."),
            );
            continue;
        }
        crate::operators::style(node, &mut group, &mut consumed, &path, report);
        mapped.extend(consumed);
        result.push(group);
    }
    Ok(result)
}

const TRANSFORM: &[(&str, &str)] = &[
    ("ADBE Vector Anchor", "anchor"),
    ("ADBE Vector Position", "position"),
    ("ADBE Vector Scale", "scale"),
    ("ADBE Vector Skew", "skew"),
    ("ADBE Vector Skew Axis", "skewAxis"),
    ("ADBE Vector Rotation", "rotation"),
    ("ADBE Vector Group Opacity", "opacity"),
];

pub(crate) fn properties(
    source: &PropertyNode,
    target: &mut PropGroup,
    mapping: &[(&str, &str)],
    ticks: u32,
    context: &str,
    mapped: &mut BTreeSet<usize>,
    report: &mut Report,
) -> Result<()> {
    for (name, path) in mapping {
        if let Some(node) = source.children.iter().find(|n| n.match_name == *name)
            && let (Some(prop), Some(out)) = (&node.property, target.prop_mut(path))
        {
            map_property(prop, out, name, ticks, &format!("{context}/{name}"), report)?;
            mapped.insert(node.source.offset);
        }
    }
    Ok(())
}

pub(crate) fn path(source: &aep_reader::BezierPath) -> Result<ShapePath> {
    let mut result = ShapePath {
        closed: source.closed.ok_or("unknown path closure")?,
        ..ShapePath::default()
    };
    for v in source
        .bounds_vertices()
        .ok_or("path has invalid/missing coordinate bounds")?
    {
        let xy = |p: [aep_reader::Number; 2]| -> Result<[f64; 2]> {
            Ok([
                crate::finite(p[0].value, "path x")?,
                crate::finite(p[1].value, "path y")?,
            ])
        };
        let anchor = xy(v.anchor)?;
        let incoming = xy(v.in_tangent)?;
        let outgoing = xy(v.out_tangent)?;
        let relative = |p: [f64; 2]| -> Result<[f64; 2]> {
            Ok([
                crate::finite(Some(p[0] - anchor[0]), "path tangent x")?,
                crate::finite(Some(p[1] - anchor[1]), "path tangent y")?,
            ])
        };
        result.vertices.push(anchor);
        result.in_tangents.push(relative(incoming)?);
        result.out_tangents.push(relative(outgoing)?);
    }
    Ok(result)
}
