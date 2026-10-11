//! Gradient snapshots and bounded midpoint approximation, without evaluating source code.
use crate::{Report, Result, shape_fields::Fields};
use aep_reader::{Gradient as Source, Number, PropertyNode};
use effectcraft_keyframe::{Gradient, Keyframe, Value};
use effectcraft_project::{PropGroup, build};
use effectcraft_time::{TICKS_PER_SECOND, Tick};
use std::collections::BTreeSet;

fn unit(n: Number, field: &str) -> Result<f64> {
    let n = crate::finite(n.value, field)?;
    if !(0.0..=1.0).contains(&n) {
        return Err(format!("{field} outside 0..1"));
    }
    Ok(n)
}

fn value(source: &Source) -> Result<(Gradient, bool)> {
    if source.color_stops.is_empty()
        || source.color_stops.len() > 2048
        || source.alpha_stops.len() > 2048
    {
        return Err("missing or excessive gradient stops".into());
    }
    let mut colors = Vec::new();
    let mut opacities = Vec::new();
    let mut approximate = false;
    for (i, stop) in source.color_stops.iter().enumerate() {
        let at = unit(stop.offset, "color stop offset")?;
        let midpoint = unit(stop.midpoint, "color midpoint")?;
        let [r, g, b] = stop.color;
        let color = [
            unit(r, "red")? as f32,
            unit(g, "green")? as f32,
            unit(b, "blue")? as f32,
            1.0,
        ];
        colors.push((at, color));
        if let Some(next) = source.color_stops.get(i + 1) {
            let end = unit(next.offset, "color stop offset")?;
            if end <= at || midpoint <= 0.0 || midpoint >= 1.0 {
                return Err("unordered stops or degenerate color midpoint".into());
            }
            // Keep one midpoint per interval even at 0.5: animated snapshots then
            // keep matching stop topology when their midpoint changes.
            let [r, g, b] = next.color;
            let other = [
                unit(r, "red")? as f32,
                unit(g, "green")? as f32,
                unit(b, "blue")? as f32,
                1.0,
            ];
            let mut half = [1.0; 4];
            for ((out, a), b) in half.iter_mut().zip(color).zip(other) {
                *out = (a + b) * 0.5;
            }
            colors.push((at + (end - at) * midpoint, half));
            approximate |= (midpoint - 0.5).abs() > 1e-7;
        }
    }
    for (i, stop) in source.alpha_stops.iter().enumerate() {
        let at = unit(stop.offset, "opacity stop offset")?;
        let alpha = unit(stop.alpha, "opacity")? as f32;
        let midpoint = unit(stop.midpoint, "opacity midpoint")?;
        opacities.push((at, alpha));
        if let Some(next) = source.alpha_stops.get(i + 1) {
            let end = unit(next.offset, "opacity stop offset")?;
            if end <= at || midpoint <= 0.0 || midpoint >= 1.0 {
                return Err("unordered stops or degenerate opacity midpoint".into());
            }
            opacities.push((
                at + (end - at) * midpoint,
                (alpha + unit(next.alpha, "opacity")? as f32) * 0.5,
            ));
            approximate |= (midpoint - 0.5).abs() > 1e-7;
        }
    }
    if opacities.is_empty() {
        return Err("missing gradient opacity stops".into());
    }
    Ok((Gradient { colors, opacities }, approximate))
}

pub(crate) fn convert(
    node: &PropertyNode,
    stroke: bool,
    ids: &mut build::Ids<'_>,
    ticks: u32,
    mapped: &mut BTreeSet<usize>,
    context: &str,
    report: &mut Report,
) -> Result<PropGroup> {
    let mut colors = node
        .children
        .iter()
        .filter(|n| n.match_name == "ADBE Vector Grad Colors");
    let colors_node = colors.next().ok_or("missing gradient colors")?;
    if colors.next().is_some() {
        return Err("duplicate gradient colors".into());
    }
    let source = colors_node
        .property
        .as_ref()
        .ok_or("missing gradient payload")?;
    if source.expression.as_ref().is_some_and(|e| !e.is_empty()) {
        return Err("gradient expression is not evaluated".into());
    }
    if source.gradient_snapshots.is_empty() || source.gradient_snapshots.len() > 4096 {
        return Err("missing or excessive gradient snapshots".into());
    }
    let mut values = Vec::new();
    let mut approximate = false;
    if source
        .gradient_snapshots
        .iter()
        .filter_map(|s| s.gradient.as_ref())
        .map(|g| g.color_stops.len() + g.alpha_stops.len())
        .sum::<usize>()
        > 32768
    {
        return Err("gradient animation exceeds stop budget".into());
    }
    for snapshot in &source.gradient_snapshots {
        let (gradient, approx) = value(
            snapshot
                .gradient
                .as_ref()
                .ok_or("undecoded gradient snapshot")?,
        )?;
        approximate |= approx;
        values.push(gradient);
    }
    if values.windows(2).any(|w| matches!(w,[a,b] if a.colors.len()!=b.colors.len() || a.opacities.len()!=b.opacities.len())) {
        return Err("changing gradient stop counts are not mapped".into());
    }
    let first = values.first().ok_or("empty gradient")?.clone();
    let animated = source.gradient_snapshots.len() > 1
        || source.gradient_snapshots.iter().any(|s| s.timing.is_some());
    if !animated && source.metadata.animated == Some(true) {
        return Err("gradient animation lacks verified timing".into());
    }
    let mut keys = Vec::new();
    if animated {
        if ticks == 0 {
            return Err("zero gradient timebase".into());
        }
        for (snapshot, value) in source.gradient_snapshots.iter().zip(values) {
            let timing = snapshot
                .timing
                .as_ref()
                .ok_or("gradient snapshot has no verified time")?;
            let t =
                i128::from(timing.time_ticks) * i128::from(TICKS_PER_SECOND) / i128::from(ticks);
            if t > i128::from(Tick::MAX.0) {
                return Err("gradient time overflow".into());
            }
            keys.push(Keyframe::new(Tick(t as i64), Value::Gradient(value)));
        }
        keys.sort_by_key(|k| k.time);
        if keys.windows(2).any(|w| matches!(w,[a,b] if a.time==b.time)) {
            return Err("duplicate gradient time".into());
        }
    }
    let mut fields = Fields {
        node,
        offsets: Vec::new(),
        defaults: Vec::new(),
    };
    let radial = match fields.scalar("ADBE Vector Grad Type", Some(1.0))? {
        1.0 => false,
        2.0 => true,
        _ => return Err("unknown gradient type".into()),
    };
    let start = fields.numbers("ADBE Vector Grad Start Pt", Some([0.0, 0.0]))?;
    let end = fields.numbers("ADBE Vector Grad End Pt", Some([100.0, 0.0]))?;
    let opacity = fields.scalar("ADBE Vector Grad Opacity", Some(100.0))?;
    let hilite = fields.scalar("ADBE Vector Grad HiLite Length", Some(0.0))?;
    let angle = fields.scalar("ADBE Vector Grad HiLite Angle", Some(0.0))?;
    let mut group = if stroke {
        let width = fields.scalar("ADBE Vector Stroke Width", Some(2.0))?;
        if width < 0.0 {
            return Err("negative gradient stroke width".into());
        }
        build::shape_gradient_stroke(ids, radial, start, end, first, width)
    } else {
        build::shape_gradient_fill(ids, radial, start, end, first)
    };
    for (name, v) in [
        ("opacity", opacity),
        ("highlightLength", hilite),
        ("highlightAngle", angle),
    ] {
        group
            .prop_mut(name)
            .ok_or("missing gradient target property")?
            .value = Value::Scalar(v);
    }
    if stroke {
        for (name, target) in [
            ("ADBE Vector Stroke Line Cap", "cap"),
            ("ADBE Vector Stroke Line Join", "join"),
        ] {
            let code = fields.scalar(name, Some(1.0))?;
            if !matches!(code, 1.0 | 2.0 | 3.0) {
                return Err(format!("unknown {name}"));
            }
            group.prop_mut(target).ok_or("missing stroke enum")?.value =
                Value::Enum(code as u32 - 1);
        }
        group.prop_mut("miter").ok_or("missing miter")?.value =
            Value::Scalar(fields.scalar("ADBE Vector Stroke Miter Limit", Some(4.0))?);
        // Nonempty advanced stroke containers cannot be represented by these fields.
        for n in &node.children {
            if matches!(
                n.match_name.as_str(),
                "ADBE Vector Stroke Dashes"
                    | "ADBE Vector Stroke Taper"
                    | "ADBE Vector Stroke Wave"
            ) && (!n.children.is_empty() || n.property.is_some())
            {
                return Err(format!("nonempty {} is not mapped", n.match_name));
            }
        }
    }
    let prop = group.prop_mut("colors").ok_or("missing colors target")?;
    report.keyframes += keys.len();
    prop.keys = keys;
    if animated {
        report.warn(context,"Gradient snapshot times and values imported with linear target interpolation; AE easing is unverified.");
    }
    if approximate {
        report.warn(context,"Gradient midpoints retained through inserted half-color/opacity stops; piecewise-linear ramp is an approximation of the AE curve.");
    }
    if !fields.defaults.is_empty() {
        report.warn(
            context,
            format!(
                "Gradient omitted fields use importer conventions: {}.",
                fields.defaults.join(", ")
            ),
        );
    }
    report.properties += fields.offsets.len() + 1;
    mapped.extend(fields.offsets);
    mapped.insert(colors_node.source.offset);
    Ok(group)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    fn source() -> Source {
        Source {
            version: None,
            color_stops: vec![
                aep_reader::GradientColorStop {
                    offset: 0.0.into(),
                    midpoint: 0.25.into(),
                    color: [1.0.into(), 0.0.into(), 0.0.into()],
                },
                aep_reader::GradientColorStop {
                    offset: 1.0.into(),
                    midpoint: 0.5.into(),
                    color: [0.0.into(), 0.0.into(), 1.0.into()],
                },
            ],
            alpha_stops: vec![
                aep_reader::GradientAlphaStop {
                    offset: 0.0.into(),
                    midpoint: 0.75.into(),
                    alpha: 0.0.into(),
                },
                aep_reader::GradientAlphaStop {
                    offset: 1.0.into(),
                    midpoint: 0.5.into(),
                    alpha: 1.0.into(),
                },
            ],
        }
    }
    #[test]
    fn midpoint_and_alpha_are_preserved_separately() {
        let (g, approx) = value(&source()).unwrap();
        assert!(approx);
        let c = g.sample(0.25);
        assert_eq!(&c[..3], &[0.5, 0.0, 0.5]);
        assert!((g.sample(0.75)[3] - 0.5).abs() < 1e-6);
    }
    #[test]
    fn bad_stops_do_not_become_default_white() {
        let mut s = source();
        s.color_stops[1].offset = 0.0.into();
        assert!(value(&s).is_err());
        s = source();
        s.alpha_stops[0].alpha = f64::NAN.into();
        assert!(value(&s).is_err());
        s = source();
        s.color_stops.clear();
        assert!(value(&s).is_err());
    }
}
