//! Experimental data-only AEP → EffectCraft adapter. No media or expression evaluation.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use aep_reader as ae;
use effectcraft_color::Label;
use effectcraft_keyframe::{Keyframe, Value};
use effectcraft_project::{
    Comp, Expression, Footage, ItemId, ItemKind, LayerSource, Project, Solid, build,
};
use effectcraft_time::{FrameRate, TICKS_PER_SECOND, Tick};
use serde::Serialize;

mod animation;
mod compositing;
mod effects;
mod gradients;
mod operators;
mod repeaters;
mod scene_metadata;
mod shape_fields;
mod shapes;
mod stars;
mod text;
mod text_animators;

pub type Result<T> = std::result::Result<T, String>;

#[derive(Debug, Serialize)]
pub struct Warning {
    pub context: String,
    pub message: String,
}

#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub compositions: usize,
    pub layers: usize,
    pub properties: usize,
    pub keyframes: usize,
    pub warnings: Vec<Warning>,
}

impl Report {
    fn warn(&mut self, context: &str, message: impl Into<String>) {
        self.warnings.push(Warning {
            context: context.into(),
            message: message.into(),
        });
    }
}

pub struct Import {
    pub project: Project,
    pub report: Report,
}

/// Repairs are explicit. The byte API is strict unless a caller opts in.
#[derive(Debug, Default, Clone, Copy)]
pub struct ImportOptions {
    pub recover_missing_parents: bool,
}

/// Parse bounded bytes into a new project. Failure never mutates an existing project.
/// Keep the original AEP: unsupported data is not round-tripped through ecproj.
pub fn import(bytes: &[u8]) -> Result<Import> {
    import_with_options(bytes, ImportOptions::default())
}

pub fn import_with_options(bytes: &[u8], options: ImportOptions) -> Result<Import> {
    // Desktop projects in the corpus exceed the reusable reader's conservative
    // defaults. Keep explicit finite budgets instead of making parsing unlimited.
    let limits = ae::Limits {
        max_chunks: 1_000_000,
        max_allocated_bytes: 768 * 1024 * 1024,
        ..ae::Limits::default()
    };
    let document = ae::parse_with_limits(bytes, &limits).map_err(|e| e.to_string())?;
    let input = &document.project;
    let mut project = Project::default();
    let mut report = Report::default();
    scene_metadata::project(input, &mut project, &mut report);
    report.warn("project", "Experimental import with per-property warnings. Color management, markers, render settings and unsupported operators are not fully mapped. Keep the original AEP; unspecified properties use target defaults.");
    for d in &input.diagnostics {
        report.warn(
            &format!("byte {}", d.offset),
            format!("{}: {}", d.code, d.message),
        );
    }
    let mut ids = BTreeMap::new();
    for item in &input.items {
        if ids.contains_key(&item.id) {
            return Err(format!("duplicate AEP item id {}", item.id));
        }
        let kind = match item.kind {
            ae::ItemKind::Folder => ItemKind::Folder,
            ae::ItemKind::Composition => {
                let c = input
                    .compositions
                    .iter()
                    .find(|c| c.id == item.id)
                    .ok_or_else(|| format!("missing composition {}", item.id))?;
                report.compositions += 1;
                ItemKind::Comp(Arc::new(composition(c)?))
            }
            _ => {
                let f = input.footage.iter().find(|f| f.id == item.id);
                if let Some(f) = f.filter(|f| f.is_solid) {
                    let color = f
                        .solid_color
                        .ok_or_else(|| format!("solid {} has no color", item.id))?;
                    let mut rgb = [0.0; 3];
                    for (target, source) in rgb.iter_mut().zip(color) {
                        *target = finite(source.value, "solid color")? as f32;
                        if !target.is_finite() {
                            return Err("solid color overflows f32".into());
                        }
                    }
                    ItemKind::Solid(Solid {
                        color: rgb,
                        width: dimension(f.width)?,
                        height: dimension(f.height)?,
                        pixel_aspect: 1.0,
                    })
                } else {
                    // No active path: opening the ecproj must not resolve paths from the AEP.
                    let reference = f
                        .and_then(|f| f.path.as_deref())
                        .unwrap_or("unknown source");
                    report.warn(
                        &item.name,
                        format!(
                            "Footage placeholder; relink manually. Original reference: {reference}"
                        ),
                    );
                    ItemKind::Footage(Footage {
                        missing: true,
                        width: f.and_then(|f| f.width).map(u32::from).unwrap_or(0),
                        height: f.and_then(|f| f.height).map(u32::from).unwrap_or(0),
                        ..Footage::default()
                    })
                }
            }
        };
        let id = project.add_item(&item.name, Label::None, None, kind);
        if let Some(out) = project.items.get_mut(&id) {
            out.comment = item.comment.clone();
        }
        ids.insert(item.id, id);
    }
    // Assign folder links only after every id exists; reject cycles before publishing output.
    for item in &input.items {
        if item.parent_id == 0 {
            continue;
        }
        let parent = ids
            .get(&item.parent_id)
            .copied()
            .ok_or("missing parent folder")?;
        if !project.item(parent).is_some_and(|p| p.is_folder()) {
            return Err("item parent is not a folder".into());
        }
        let id = *ids.get(&item.id).ok_or("missing item id")?;
        if let Some(out) = project.items.get_mut(&id) {
            out.parent = Some(parent);
        }
    }
    validate_chains(project.items.values().map(|i| (i.id, i.parent)).collect())?;
    for source_comp in &input.compositions {
        let cid = *ids.get(&source_comp.id).ok_or("missing composition id")?;
        let mut comp = project
            .comp(cid)
            .ok_or("missing target composition")?
            .clone();
        scene_metadata::composition(source_comp, &mut comp, &mut report)?;
        let mut layer_ids = BTreeMap::new();
        for source in &source_comp.layers {
            if layer_ids.contains_key(&source.id) {
                return Err("duplicate layer id".into());
            }
            let context = format!("{}/{}", source_comp.name, source.name);
            let layer_source = match source.kind {
                ae::LayerKind::Null => LayerSource::Null,
                ae::LayerKind::Shape => LayerSource::Shape,
                ae::LayerKind::Camera => LayerSource::Camera,
                ae::LayerKind::Light => match scene_metadata::light(source.light_kind) {
                    Some(kind) => LayerSource::Light { kind },
                    None => {
                        report.warn(&context, "Unknown light type; null placeholder retained.");
                        LayerSource::Null
                    }
                },
                ae::LayerKind::Text if source.text_source.is_some() => LayerSource::Text,
                ae::LayerKind::Av | ae::LayerKind::Adjustment => {
                    let item = source.source_id.and_then(|id| ids.get(&id)).copied();
                    match item.and_then(|id| project.item(id).map(|item| (id, &item.kind))) {
                        Some((item, ItemKind::Comp(_))) => LayerSource::Comp { item },
                        Some((item, ItemKind::Solid(_))) => LayerSource::Solid { item },
                        Some((item, ItemKind::Footage(_))) => LayerSource::Footage { item },
                        _ => {
                            report.warn(&context, "Missing layer source; imported as a null.");
                            LayerSource::Null
                        }
                    }
                }
                other => {
                    report.warn(
                        &context,
                        format!("{other:?} content is not mapped; imported as a null."),
                    );
                    LayerSource::Null
                }
            };
            let size = layer_source
                .item()
                .and_then(|id| project.item(id))
                .and_then(|i| i.dimensions())
                .unwrap_or((100, 100));
            let mut layer =
                build::layer(&mut project, &comp, &source.name, layer_source, size, None);
            layer.name = source.name.clone();
            layer.comment = source.comment.clone();
            let start = ratio(source.start_time, "start time")?;
            let stretch = ratio(source.stretch, "stretch")?;
            if stretch == 0.0 {
                return Err(format!("{context}: zero stretch"));
            }
            layer.start_time = seconds(start)?;
            layer.stretch = finite(Some(stretch * 100.0), "stretch percent")?;
            let a = seconds(start + ratio(source.source_in_point, "in point")? * stretch)?;
            let b = seconds(start + ratio(source.source_out_point, "out point")? * stretch)?;
            layer.in_point = a.min(b);
            layer.out_point = a.max(b);
            if start != 0.0 || stretch != 1.0 {
                report.warn(&context, "Non-default start/stretch mapped from source time; visual timing still needs AE comparison.");
            }
            if let Some(s) = &source.switches {
                layer.switches.video = s.visible;
                layer.switches.audio = s.audio;
                layer.switches.locked = s.locked;
                layer.switches.shy = s.shy;
                layer.switches.solo = s.solo;
                layer.switches.three_d = s.three_d;
                layer.switches.guide = s.guide;
                layer.switches.adjustment = s.adjustment;
            }
            layer.blend_mode = compositing::blend(source.blend_mode, &context, &mut report);
            if let Some(value) = source.preserve_transparency {
                layer.preserve_transparency = value;
            }
            let mut mapped_nodes = BTreeSet::new();
            if let Some(document) = &source.text_source {
                let (converted, warnings) = text::convert(document)?;
                if let Some(property) = layer.props.prop_mut("text/sourceText") {
                    property.value = Value::Text(Box::new(converted));
                }
                for message in warnings {
                    report.warn(&context, message);
                }
                report.warn(&context, "Static source text imported; source-text content animation and unsupported animator variants are not mapped.");
                if let (Some(root), Some(target)) = (
                    source.node("ADBE Text Animators"),
                    layer.props.group_mut("text/animators"),
                ) {
                    let mut allocated = build::Ids(&mut project.next_id);
                    target.children = text_animators::convert(
                        root,
                        &mut allocated,
                        source_comp.keyframe_ticks_per_second,
                        &context,
                        &mut mapped_nodes,
                        &mut report,
                    )?
                    .into_iter()
                    .map(Into::into)
                    .collect();
                }
            }
            let transform_nodes = source
                .properties
                .iter()
                .find(|n| n.match_name == "ADBE Transform Group")
                .map(|n| n.children.as_slice())
                .unwrap_or(&source.properties);
            for (name, path) in TRANSFORMS {
                if let Some(node) = transform_nodes.iter().find(|n| n.match_name == *name) {
                    let Some(prop) = &node.property else {
                        continue;
                    };
                    let Some(target) = layer.props.prop_mut(path) else {
                        continue;
                    };
                    map_property(
                        prop,
                        target,
                        name,
                        source_comp.keyframe_ticks_per_second,
                        &format!("{context}/{name}"),
                        &mut report,
                    )?;
                    mapped_nodes.insert(node.source.offset);
                }
            }
            animation::channels(
                source,
                &mut layer,
                &mut build::Ids(&mut project.next_id),
                source_comp.keyframe_ticks_per_second,
                &mut mapped_nodes,
                &context,
                &mut report,
            )?;
            scene_metadata::layer(
                source,
                &mut layer,
                source_comp.keyframe_ticks_per_second,
                &mut mapped_nodes,
                &context,
                &mut report,
            )?;
            if let Some(root) = source.shapes() {
                let mut allocated = build::Ids(&mut project.next_id);
                let groups = shapes::contents(
                    &root.node.children,
                    &mut allocated,
                    source_comp.keyframe_ticks_per_second,
                    &context,
                    &mut mapped_nodes,
                    &mut report,
                )?;
                if let Some(contents) = layer.props.group_mut("contents") {
                    contents.children = groups.into_iter().map(Into::into).collect();
                    if root.node.enabled == Some(false) {
                        report.warn(&context, "Disabled shape root propagated to child groups; original per-child flags remain in AEP.");
                        for child in &mut contents.children {
                            if let effectcraft_project::Node::Group(group) = child {
                                group.enabled = false;
                            }
                        }
                    }
                }
            }
            let mask_scale = match &layer.source {
                LayerSource::Solid { item }
                | LayerSource::Comp { item }
                | LayerSource::Footage { item } => project
                    .item(*item)
                    .and_then(|i| i.dimensions())
                    .filter(|(w, h)| *w > 0 && *h > 0)
                    .map(|(w, h)| [f64::from(w), f64::from(h)]),
                _ => Some([1.0, 1.0]),
            };
            if let (Some(root), Some(target), Some(scale)) = (
                source.node("ADBE Mask Parade"),
                layer.props.group_mut("masks"),
                mask_scale,
            ) {
                let mut allocated = build::Ids(&mut project.next_id);
                target.children = shapes::masks(
                    &root.children,
                    &mut allocated,
                    source_comp.keyframe_ticks_per_second,
                    &context,
                    scale,
                    &mut mapped_nodes,
                    &mut report,
                )?
                .into_iter()
                .map(Into::into)
                .collect();
                if root.enabled == Some(false) {
                    report.warn(&context, "Disabled mask stack propagated to masks; original per-mask flags remain in AEP.");
                    for child in &mut target.children {
                        if let effectcraft_project::Node::Group(group) = child {
                            group.enabled = false;
                        }
                    }
                }
            }
            layer.switches.effects = source.switches.as_ref().is_some_and(|s| s.effects)
                && source
                    .node("ADBE Effect Parade")
                    .and_then(|n| n.enabled)
                    .unwrap_or(source.effects().is_empty());
            if !source.effects().is_empty()
                && (source.switches.is_none()
                    || source
                        .node("ADBE Effect Parade")
                        .and_then(|n| n.enabled)
                        .is_none())
            {
                report.warn(
                    &context,
                    "Layer effect enable state unknown; effect stack imported disabled.",
                );
            }
            if let Some(target) = layer.props.group_mut("effects") {
                target.enabled = layer.switches.effects;
                let mut allocated = build::Ids(&mut project.next_id);
                target.children = effects::convert(
                    input,
                    source.effects(),
                    &mut allocated,
                    source_comp.keyframe_ticks_per_second,
                    &context,
                    &mut mapped_nodes,
                    &mut report,
                )?
                .into_iter()
                .map(Into::into)
                .collect();
            }
            report_unmapped(&source.properties, &context, &mapped_nodes, &mut report);
            layer_ids.insert(source.id, layer.id);
            comp.layers.push(layer);
            report.layers += 1;
        }
        for (source, target) in source_comp.layers.iter().zip(&mut comp.layers) {
            if let Some(id) = source.parent_id.filter(|id| *id != 0) {
                if let Some(parent) = layer_ids.get(&id) {
                    target.parent = Some(*parent);
                } else if options.recover_missing_parents {
                    report.warn(&format!("{}/{}", source_comp.name, source.name), format!("Missing parent layer ID {id}; imported without parenting. Saved local transforms retained; original world-space placement cannot be recovered."));
                } else {
                    return Err("missing layer parent".into());
                }
            }
        }
        compositing::mattes(source_comp, &mut comp, &layer_ids, &mut report)?;
        validate_chains(comp.layers.iter().map(|l| (l.id, l.parent)).collect())?;
        *project.comp_mut(cid).ok_or("missing composition")? = comp;
    }
    validate_precomps(&project)?;
    Ok(Import { project, report })
}

const TRANSFORMS: &[(&str, &str)] = &[
    ("ADBE Anchor Point", "transform/anchor"),
    ("ADBE Position", "transform/position"),
    ("ADBE Scale", "transform/scale"),
    ("ADBE Rotate Z", "transform/rotation"),
    ("ADBE Rotate X", "transform/rotationX"),
    ("ADBE Rotate Y", "transform/rotationY"),
    ("ADBE Orientation", "transform/orientation"),
    ("ADBE Opacity", "transform/opacity"),
];

fn finite(v: Option<f64>, field: &str) -> Result<f64> {
    v.filter(|v| v.is_finite())
        .ok_or_else(|| format!("missing or non-finite {field}"))
}
fn ratio(v: Option<ae::Ratio>, field: &str) -> Result<f64> {
    finite(v.and_then(|v| v.value()), field)
}
fn dimension(v: Option<u16>) -> Result<u32> {
    v.filter(|v| *v > 0)
        .map(u32::from)
        .ok_or_else(|| "missing or zero dimension".into())
}
fn seconds(s: f64) -> Result<Tick> {
    let t = s * TICKS_PER_SECOND as f64;
    if !t.is_finite() || t.abs() >= Tick::MAX.0 as f64 {
        return Err("time exceeds EffectCraft range".into());
    }
    Ok(Tick(t.round() as i64))
}
fn composition(c: &ae::Composition) -> Result<Comp> {
    let fixed = c
        .frame_rate_fixed
        .filter(|v| *v > 0)
        .ok_or("missing frame rate")?;
    let duration = seconds(finite(c.duration_seconds, "duration")?)?;
    if duration <= Tick::ZERO {
        return Err("non-positive composition duration".into());
    }
    let mut out = Comp::new(
        dimension(c.width)?,
        dimension(c.height)?,
        FrameRate::new(i64::from(fixed), 65536),
        duration,
    );
    out.background = c
        .background_rgb
        .unwrap_or([0; 3])
        .map(|v| f32::from(v) / 255.0);
    if let Some(v) = c.pixel_aspect {
        out.pixel_aspect = ratio(Some(v), "pixel aspect")?;
        if out.pixel_aspect <= 0.0 {
            return Err("non-positive pixel aspect".into());
        }
    }
    if let Some(v) = c.display_start {
        out.display_start = seconds(ratio(Some(v), "display start")?)?;
    }
    Ok(out)
}

fn numeric(v: &ae::Value, target: &Value, factor: f64) -> Result<Value> {
    if let (ae::Value::Path(path), Value::Path(_)) = (v, target) {
        return Ok(Value::Path(shapes::path(path)?));
    }
    let n = v.numbers().ok_or("expected numeric transform")?;
    let values: Vec<f64> = n
        .iter()
        .map(|n| finite(n.value.map(|v| v * factor), "property value"))
        .collect::<Result<_>>()?;
    match (values.as_slice(), target) {
        ([x], Value::Scalar(_)) => Ok(Value::Scalar(*x)),
        ([x], Value::Bool(_)) if matches!(*x, 0.0 | 1.0) => Ok(Value::Bool(*x == 1.0)),
        ([x, y, z], Value::Vec3(_)) => Ok(Value::Vec3([*x, *y, *z])),
        ([x, y], Value::Vec2(_)) | ([x, y, _], Value::Vec2(_)) => Ok(Value::Vec2([*x, *y])),
        // AEP numeric colors are stored ARGB in 0..255, not scripting RGBA.
        ([a, r, g, b], Value::Color(_)) => {
            Ok(Value::Color([r / 255.0, g / 255.0, b / 255.0, a / 255.0]))
        }
        _ => Err("transform dimensions do not match target".into()),
    }
}
fn map_property(
    source: &ae::Property,
    target: &mut effectcraft_project::Property,
    match_name: &str,
    ticks: u32,
    context: &str,
    report: &mut Report,
) -> Result<()> {
    let factor = match match_name {
        "ADBE Scale" | "ADBE Opacity" | "ADBE Mask Opacity" => 100.0,
        _ => 1.0,
    };
    if let Some(v) = &source.static_value {
        target.value = numeric(v, &target.value, factor)?;
    } else {
        report.warn(
            context,
            "No saved static value; EffectCraft's default is retained.",
        );
    }
    for key in &source.keyframes {
        if ticks == 0 {
            return Err("zero keyframe timebase".into());
        }
        let time = (i128::from(key.time_ticks) * i128::from(TICKS_PER_SECOND)) / i128::from(ticks);
        if time > i128::from(Tick::MAX.0) {
            return Err("keyframe time overflow".into());
        }
        let Some(source_value) = key.value.as_ref() else {
            report.warn(context, format!("Undecoded keyframe value at tick {}; key omitted; remaining decoded keys and static/default value retained.", key.time_ticks));
            continue;
        };
        let value = numeric(source_value, &target.value, factor)?;
        let mut mapped = Keyframe::new(Tick(time as i64), value);
        mapped.spatial_auto = false;
        // Only code 1 has independent authored-file evidence so far. Unknown
        // modes remain explicit approximations, while their saved ease is retained.
        if key.in_interpolation != 1 || key.out_interpolation != 1 {
            report.warn(context, format!("Unverified interpolation codes {}/{} at tick {}; interpolation is linear as a fallback.", key.in_interpolation, key.out_interpolation, key.time_ticks));
        }
        let ease = |values: &[ae::TemporalEase]| -> Result<Vec<effectcraft_keyframe::Ease>> {
            values
                .iter()
                .map(|e| {
                    let influence = finite(e.influence.value, "ease influence")?;
                    if !(0.0..=1.0).contains(&influence) {
                        return Err("ease influence outside 0..1".into());
                    }
                    Ok(effectcraft_keyframe::Ease {
                        speed: finite(e.speed.value.map(|v| v * factor), "ease speed")?,
                        influence,
                    })
                })
                .collect()
        };
        match (ease(&key.in_ease), ease(&key.out_ease)) {
            (Ok(in_ease), Ok(out_ease)) => {
                mapped.in_ease = in_ease;
                mapped.out_ease = out_ease;
            }
            (Err(reason), _) | (_, Err(reason)) => {
                report.warn(context, format!("Invalid temporal ease at tick {}: {reason}; handles omitted, key time/value retained with linear interpolation.", key.time_ticks));
            }
        }
        let tangent = |values: &[ae::Number]| -> Result<[f64; 3]> {
            if !values.is_empty() && values.len() != 2 && values.len() != 3 {
                return Err("invalid spatial tangent dimensions".into());
            }
            let mut out = [0.0; 3];
            for (out, value) in out.iter_mut().zip(values) {
                *out = finite(value.value, "spatial tangent")?;
            }
            Ok(out)
        };
        if target.spatial {
            mapped.spatial_in = tangent(&key.in_spatial_tangent)?;
            mapped.spatial_out = tangent(&key.out_spatial_tangent)?;
        } else if key
            .in_spatial_tangent
            .iter()
            .chain(&key.out_spatial_tangent)
            .any(|v| v.value != Some(0.0))
        {
            report.warn(
                context,
                "Stored tangent fields on a non-spatial property are not applied.",
            );
        }
        if key.raw_flags[0] != 0 || !matches!(key.raw_flags[1], 0 | 1 | 7) {
            report.warn(context, format!("Unverified keyframe flags {:?}; saved handles retained without inferring auto/roving behavior.", key.raw_flags));
        }
        target.keys.push(mapped);
        report.keyframes += 1;
    }
    target.keys.sort_by_key(|k| k.time);
    if target
        .keys
        .windows(2)
        .any(|p| matches!(p, [a, b] if a.time == b.time))
    {
        return Err("duplicate keyframe time".into());
    }
    if let Some(text) = &source.expression {
        target.expr = Some(Expression {
            text: text.clone(),
            enabled: false,
        });
        report.warn(context, "Expression preserved but disabled.");
    }
    report.properties += 1;
    Ok(())
}

fn report_unmapped(
    nodes: &[ae::PropertyNode],
    context: &str,
    mapped: &BTreeSet<usize>,
    report: &mut Report,
) {
    // Parser limits already bound the tree depth.
    for node in nodes {
        let path = format!("{context}/{}[{}]", node.match_name, node.index);
        if node.decode_status != ae::DecodeStatus::Decoded {
            report.warn(
                &path,
                format!(
                    "Reader status {:?}; original data at byte {}.",
                    node.decode_status, node.source.offset
                ),
            );
        }
        if node.property.is_some() && !mapped.contains(&node.source.offset) {
            report.warn(
                &path,
                format!(
                    "Property not mapped; original data at byte {}.",
                    node.source.offset
                ),
            );
        }
        report_unmapped(&node.children, &path, mapped, report);
    }
}

fn validate_chains<T: Copy + Ord>(links: BTreeMap<T, Option<T>>) -> Result<()> {
    let mut done = BTreeSet::new();
    for &id in links.keys() {
        let mut seen = BTreeSet::new();
        let mut current = Some(id);
        while let Some(id) = current {
            if done.contains(&id) {
                break;
            }
            if !seen.insert(id) {
                return Err("cyclic parent links".into());
            }
            current = *links.get(&id).ok_or("dangling parent link")?;
        }
        done.extend(seen);
    }
    Ok(())
}

fn validate_precomps(project: &Project) -> Result<()> {
    // Kahn's algorithm bounds work even on a malicious graph with shared subcompositions.
    let mut counts: BTreeMap<ItemId, usize> = project
        .items
        .iter()
        .filter_map(|(id, i)| i.as_comp().map(|_| (*id, 0)))
        .collect();
    let mut edges: BTreeMap<ItemId, Vec<ItemId>> = BTreeMap::new();
    for (&id, item) in &project.items {
        if let Some(comp) = item.as_comp() {
            for layer in &comp.layers {
                if let LayerSource::Comp { item } = layer.source {
                    let count = counts.get_mut(&item).ok_or("dangling precomp")?;
                    *count = count.checked_add(1).ok_or("too many precomp links")?;
                    edges.entry(id).or_default().push(item);
                }
            }
        }
    }
    let mut ready: Vec<ItemId> = counts
        .iter()
        .filter_map(|(id, count)| (*count == 0).then_some(*id))
        .collect();
    let mut visited = 0;
    while let Some(id) = ready.pop() {
        visited += 1;
        for child in edges.get(&id).into_iter().flatten() {
            let count = counts.get_mut(child).ok_or("missing precomp")?;
            *count = count.checked_sub(1).ok_or("invalid precomp graph")?;
            if *count == 0 {
                ready.push(*child);
            }
        }
    }
    if visited != counts.len() {
        return Err("cyclic precomp links".into());
    }
    Ok(())
}
