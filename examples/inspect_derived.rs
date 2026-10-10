//! Local comparison surface for the reader's derived, borrowed accessors.
use aep_reader::{
    AvItemRef, DefaultValue, Footage, Layer, NameSource, PropertyNode, PropertyValueType, Value,
    Vertex, known_property_units,
};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    env,
    fs::File,
    io::{self, Read},
};

#[derive(Serialize)]
struct Node<'a> {
    path: String,
    name: &'a str,
    name_source: NameSource,
    is_name_set: bool,
    explicit_name: bool,
    units: Option<&'static str>,
    default_value: Option<DefaultValue<'a>>,
    modified: Option<bool>,
    control_type: Option<u8>,
    value_type: Option<PropertyValueType>,
    geometry: Vec<PathView>,
}
#[derive(Serialize)]
struct PathView {
    time_ticks: Option<u32>,
    associated: bool,
    closed: Option<bool>,
    layer_vertices: Option<Vec<Vertex>>,
}
fn path_view(path: &aep_reader::BezierPath, time_ticks: Option<u32>, associated: bool) -> PathView {
    PathView {
        time_ticks,
        associated,
        closed: path.closed,
        layer_vertices: path.layer_vertices().map(Iterator::collect),
    }
}
fn nodes<'a>(entries: &'a [PropertyNode], prefix: &str, out: &mut Vec<Node<'a>>) {
    let mut occurrences = BTreeMap::new();
    for (i, n) in entries.iter().enumerate() {
        let count = occurrences.entry(n.match_name.as_str()).or_insert(0usize);
        let occurrence = *count;
        *count += 1;
        let path = format!("{prefix}/{i}:{}#{occurrence}", n.match_name);
        let mut geometry = Vec::new();
        if let Some(p) = &n.property {
            if let Some(Value::Path(path)) = &p.static_value {
                geometry.push(path_view(path, None, true));
            }
            for k in &p.keyframes {
                if let Some(Value::Path(path)) = &k.value {
                    geometry.push(path_view(path, Some(k.time_ticks), true));
                }
            }
            for s in &p.unmatched_path_snapshots {
                if let Some(path) = &s.path {
                    geometry.push(path_view(path, None, false));
                }
            }
        }
        out.push(Node {
            path: path.clone(),
            name: &n.name,
            name_source: n.name_source,
            is_name_set: n.is_name_set(),
            explicit_name: n.has_explicit_name(),
            units: n.units().map(|u| u.as_str()),
            default_value: n.property.as_ref().and_then(|p| p.default_value()),
            modified: n.is_modified(),
            control_type: n.property.as_ref().and_then(|p| p.metadata.control_type),
            value_type: n.property.as_ref().map(|p| p.metadata.value_type),
            geometry,
        });
        nodes(&n.children, &path, out);
    }
}
#[derive(Serialize)]
struct LayerView<'a> {
    id: u32,
    parent_id: Option<u32>,
    source_id: Option<u32>,
    alternate_source_id: Option<u32>,
    explicit_matte_id: Option<u32>,
    light_source_id: Option<u32>,
    time_remap_enabled: bool,
    has_alternate_source_slot: bool,
    nodes: Vec<Node<'a>>,
}
fn layer<'a>(l: &'a Layer, c: &aep_reader::Composition, p: &aep_reader::Project) -> LayerView<'a> {
    let mut out = Vec::new();
    nodes(&l.properties, "", &mut out);
    LayerView {
        id: l.id,
        parent_id: c.parent_of(l).map(|l| l.id),
        source_id: p.source_of(l).map(AvItemRef::id),
        alternate_source_id: p.alternate_source_of(l).map(AvItemRef::id),
        explicit_matte_id: c.explicit_matte_source_of(l).map(|l| l.id),
        light_source_id: c.light_source_of(l).map(|l| l.id),
        time_remap_enabled: l.time_remap_enabled(),
        has_alternate_source_slot: l.has_alternate_source_slot(),
        nodes: out,
    }
}
#[derive(Serialize)]
struct CompView<'a> {
    id: u32,
    active_camera_id: Option<u32>,
    layers: Vec<LayerView<'a>>,
}
#[derive(Serialize)]
struct FootageView {
    id: u32,
    kind: &'static str,
    has_audio: Option<bool>,
    missing_at_save: Option<bool>,
}
fn footage(f: &Footage) -> FootageView {
    FootageView {
        id: f.id,
        kind: f.main_source().kind(),
        has_audio: f.has_audio(),
        missing_at_save: f.file_missing_at_save(),
    }
}
#[derive(Serialize)]
struct Summary<'a> {
    compositions: Vec<CompView<'a>>,
    footage: Vec<FootageView>,
    catalog: Vec<(&'static str, &'static str)>,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args_os()
        .nth(1)
        .ok_or("usage: inspect_derived project.aep")?;
    let limits = aep_reader::Limits::default();
    let mut bytes = Vec::new();
    File::open(path)?
        .take(u64::try_from(limits.max_input_bytes)?.saturating_add(1))
        .read_to_end(&mut bytes)?;
    let doc = aep_reader::parse_with_limits(&bytes, &limits)?;
    let comps = doc
        .project
        .compositions
        .iter()
        .map(|c| CompView {
            id: c.id,
            active_camera_id: c.active_camera().map(|l| l.id),
            layers: c.layers.iter().map(|l| layer(l, c, &doc.project)).collect(),
        })
        .collect();
    serde_json::to_writer(
        io::stdout().lock(),
        &Summary {
            compositions: comps,
            footage: doc.project.footage.iter().map(footage).collect(),
            catalog: known_property_units()
                .map(|(n, u)| (n, u.as_str()))
                .collect(),
        },
    )?;
    Ok(())
}
