//! Bounded static path modifiers. Target algorithms are approximations, not AE renderers.
use std::collections::BTreeSet;

use aep_reader::{PropertyNode, ShapeKind};
use effectcraft_keyframe::Value;
use effectcraft_project::{PropGroup, build};

use crate::{Report, Result, shape_fields::Fields};

/// Conservative whole-layer bound, including nested groups and animated path snapshots.
/// Multiplication across independent branches deliberately overestimates expansion.
pub(crate) fn within_budget(nodes: &[PropertyNode]) -> bool {
    fn visit(nodes: &[PropertyNode], vertices: &mut usize, factor: &mut usize) {
        for n in nodes {
            let kind = ShapeKind::from_match_name(&n.match_name);
            *vertices = vertices.saturating_add(match kind {
                ShapeKind::Rectangle | ShapeKind::Ellipse => 8,
                ShapeKind::Star => {
                    let mut f = Fields {
                        node: n,
                        offsets: Vec::new(),
                        defaults: Vec::new(),
                    };
                    f.scalar("ADBE Vector Star Points", Some(5.0))
                        .ok()
                        .filter(|v| (3.0..=1024.0).contains(v) && v.fract() == 0.0)
                        .map_or(4096, |v| v as usize * 4)
                }
                _ => 0,
            });
            *factor = factor.saturating_mul(match kind {
                ShapeKind::RoundCorners => 4,
                ShapeKind::PuckerBloat => 2,
                ShapeKind::Twist => 64,
                ShapeKind::ZigZag => 66,
                ShapeKind::Repeater => 128,
                ShapeKind::MergePaths => 16,
                ShapeKind::OffsetPaths => 16,
                _ => 1,
            });
            if let Some(p) = &n.property {
                let max = p
                    .static_value
                    .iter()
                    .chain(p.keyframes.iter().filter_map(|k| k.value.as_ref()))
                    .filter_map(|v| match v {
                        aep_reader::Value::Path(p) => Some(p.vertices.len()),
                        _ => None,
                    })
                    .max()
                    .unwrap_or(0);
                *vertices = vertices.saturating_add(max);
            }
            visit(&n.children, vertices, factor);
        }
    }
    let (mut vertices, mut factor) = (0, 1);
    visit(nodes, &mut vertices, &mut factor);
    vertices <= 4096 && vertices.saturating_mul(factor) <= 262_144
}

pub(crate) fn convert(
    node: &PropertyNode,
    ids: &mut build::Ids<'_>,
    mapped: &mut BTreeSet<usize>,
    ticks: u32,
    context: &str,
    report: &mut Report,
) -> Result<PropGroup> {
    let mut f = Fields {
        node,
        offsets: Vec::new(),
        defaults: Vec::new(),
    };
    let mut animated = Vec::new();
    let (kind, values) = match ShapeKind::from_match_name(&node.match_name) {
        ShapeKind::RoundCorners => {
            let radius = animated_scalar(
                &mut f,
                "ADBE Vector RoundCorner Radius",
                10.0,
                0.0,
                100000.0,
                &mut animated,
            )?;
            ("round", vec![("radius", Value::Scalar(radius))])
        }
        ShapeKind::PuckerBloat => {
            let amount = animated_scalar(
                &mut f,
                "ADBE Vector PuckerBloat Amount",
                0.0,
                -100.0,
                100.0,
                &mut animated,
            )?;
            ("pucker", vec![("amount", Value::Scalar(amount))])
        }
        ShapeKind::Twist => {
            let angle = animated_scalar(
                &mut f,
                "ADBE Vector Twist Angle",
                0.0,
                -3600.0,
                3600.0,
                &mut animated,
            )?;
            let center = f.numbers("ADBE Vector Twist Center", Some([0.0; 2]))?;
            (
                "twist",
                vec![
                    ("angle", Value::Scalar(angle)),
                    ("center", Value::Vec2(center)),
                ],
            )
        }
        ShapeKind::ZigZag => {
            let size = animated_scalar(
                &mut f,
                "ADBE Vector Zigzag Size",
                5.0,
                0.0,
                100000.0,
                &mut animated,
            )?;
            let ridges = bounded(
                f.scalar("ADBE Vector Zigzag Detail", Some(10.0))?,
                0.0,
                32.0,
            )?;
            if ridges.fract() != 0.0 {
                return Err("fractional zigzag ridges require different geometry".into());
            }
            let points = popup(f.scalar("ADBE Vector Zigzag Points", Some(1.0))?, 2)?;
            (
                "zigzag",
                vec![
                    ("size", Value::Scalar(size)),
                    ("ridges", Value::Scalar(ridges)),
                    ("points", points),
                ],
            )
        }
        ShapeKind::MergePaths => {
            let mode = popup(f.scalar("ADBE Vector Merge Type", Some(1.0))?, 5)?;
            ("merge", vec![("mode", mode)])
        }
        ShapeKind::OffsetPaths => {
            let amount = animated_scalar(
                &mut f,
                "ADBE Vector Offset Amount",
                10.0,
                -100000.0,
                100000.0,
                &mut animated,
            )?;
            let join = popup(f.scalar("ADBE Vector Offset Line Join", Some(1.0))?, 3)?;
            let miter = bounded(
                f.scalar("ADBE Vector Offset Miter Limit", Some(4.0))?,
                1.0,
                100.0,
            )?;
            let copies = f.scalar("ADBE Vector Offset Copies", Some(1.0))?;
            let offset = f.scalar("ADBE Vector Offset Copy Offset", Some(1.0))?;
            if copies != 1.0 || offset != 1.0 {
                return Err("offset copies/copy-offset semantics differ from target; only one copy with default step is mapped".into());
            }
            (
                "offset",
                vec![
                    ("amount", Value::Scalar(amount)),
                    ("join", join),
                    ("miter", Value::Scalar(miter)),
                    ("copies", Value::Scalar(1.0)),
                    ("copyOffset", Value::Scalar(0.0)),
                ],
            )
        }
        _ => return Err("unsupported operator".into()),
    };
    let mut group = build::shape_simple_op(ids, kind).ok_or("missing target operator")?;
    for (name, value) in values {
        group
            .prop_mut(name)
            .ok_or("missing target operator property")?
            .value = value;
    }
    let mut animation_report = Report::default();
    for (name, source) in animated {
        let path = match name {
            "ADBE Vector RoundCorner Radius" => "radius",
            "ADBE Vector PuckerBloat Amount" | "ADBE Vector Offset Amount" => "amount",
            "ADBE Vector Twist Angle" => "angle",
            "ADBE Vector Zigzag Size" => "size",
            _ => return Err("unknown animated operator field".into()),
        };
        crate::map_property(
            source,
            group
                .prop_mut(path)
                .ok_or("missing target animated field")?,
            name,
            ticks,
            context,
            &mut animation_report,
        )?;
    }
    report.keyframes += animation_report.keyframes;
    report.warnings.extend(animation_report.warnings);
    if !f.defaults.is_empty() {
        report.warn(
            context,
            format!(
                "Omitted operator fields use explicit importer conventions: {}.",
                f.defaults.join(", ")
            ),
        );
    }
    report.warn(
        context,
        "Path modifier uses the target geometry algorithm; AE contour equivalence is unverified.",
    );
    report.properties += f.offsets.len();
    mapped.extend(f.offsets);
    Ok(group)
}

fn bounded(value: f64, min: f64, max: f64) -> Result<f64> {
    if (min..=max).contains(&value) {
        Ok(value)
    } else {
        Err("operator value exceeds verified geometry range".into())
    }
}

fn popup(value: f64, max: u32) -> Result<Value> {
    if value.fract() == 0.0 && (1.0..=f64::from(max)).contains(&value) {
        Ok(Value::Enum(value as u32 - 1))
    } else {
        Err("unknown operator enumeration".into())
    }
}

/// Static popup values must be converted explicitly; numeric keyframes are not enum keys.
pub(crate) fn style(
    source: &PropertyNode,
    target: &mut PropGroup,
    mapped: &mut BTreeSet<usize>,
    context: &str,
    report: &mut Report,
) {
    let fields: &[(&str, &str, u32)] = match ShapeKind::from_match_name(&source.match_name) {
        ShapeKind::Stroke => &[
            ("ADBE Vector Stroke Line Cap", "cap", 3),
            ("ADBE Vector Stroke Line Join", "join", 3),
        ],
        ShapeKind::Fill => &[("ADBE Vector Fill Rule", "rule", 2)],
        ShapeKind::TrimPaths => &[("ADBE Vector Trim Type", "mode", 2)],
        _ => &[],
    };
    for &(name, path, max) in fields {
        if !source.children.iter().any(|n| n.match_name == name) {
            continue;
        }
        let mut f = Fields {
            node: source,
            offsets: Vec::new(),
            defaults: Vec::new(),
        };
        match f.scalar(name, None).and_then(|v| popup(v, max)) {
            Ok(value) => {
                if let Some(p) = target.prop_mut(path) {
                    p.value = value;
                    report.properties += f.offsets.len();
                    mapped.extend(f.offsets);
                }
            }
            Err(reason) => report.warn(
                context,
                format!("{name}: {reason}; target default retained."),
            ),
        }
    }
}

fn animated_scalar<'a>(
    f: &mut Fields<'a>,
    name: &'static str,
    default: f64,
    min: f64,
    max: f64,
    animated: &mut Vec<(&'static str, &'a aep_reader::Property)>,
) -> Result<f64> {
    let mut matches = f.node.children.iter().filter(|n| n.match_name == name);
    if let Some(node) = matches.next()
        && let Some(p) = &node.property
        && p.is_animated()
    {
        if matches.next().is_some()
            || p.keyframes.is_empty()
            || p.keyframes.len() > 4096
            || p.expression.as_ref().is_some_and(|s| !s.is_empty())
        {
            return Err(format!("invalid/expressed animated {name}"));
        }
        let mut first = None;
        for k in &p.keyframes {
            let values = k
                .value
                .as_ref()
                .and_then(|v| v.numbers())
                .ok_or("incomplete operator keyframe")?;
            let [number] = values else {
                return Err("invalid operator keyframe dimensions".into());
            };
            let value = bounded(crate::finite(number.value, name)?, min, max)?;
            first.get_or_insert(value);
        }
        f.offsets.push(node.source.offset);
        animated.push((name, p));
        return first.ok_or_else(|| "missing operator keys".into());
    }
    bounded(f.scalar(name, Some(default))?, min, max)
}
