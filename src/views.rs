//! Borrowed lookups and views over saved metadata. No indexes, backrefs or I/O.
use crate::{Composition, Footage, Item, ItemKind, Layer, LayerKind, Number, Project};

#[derive(Debug, Clone, Copy)]
pub enum AvItemRef<'a> {
    Composition(&'a Composition),
    Footage(&'a Footage),
}
impl<'a> AvItemRef<'a> {
    pub fn id(self) -> u32 {
        match self {
            Self::Composition(c) => c.id,
            Self::Footage(f) => f.id,
        }
    }
    pub fn name(self) -> &'a str {
        match self {
            Self::Composition(c) => &c.name,
            Self::Footage(f) => &f.name,
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub enum FootageSourceRef<'a> {
    File {
        path: Option<&'a str>,
        sequence_members: &'a [String],
    },
    Solid {
        color: Option<&'a [Number; 3]>,
    },
    Placeholder,
}
impl FootageSourceRef<'_> {
    pub fn kind(self) -> &'static str {
        match self {
            Self::File { .. } => "file",
            Self::Solid { .. } => "solid",
            Self::Placeholder => "placeholder",
        }
    }
}
fn reference(id: u32) -> Option<u32> {
    (id != 0 && id != u32::MAX).then_some(id)
}
impl Project {
    pub fn composition_by_name(&self, name: &str) -> Option<&Composition> {
        self.compositions.iter().find(|c| c.name == name)
    }
    pub fn footage_by_name(&self, name: &str) -> Option<&Footage> {
        self.footage.iter().find(|f| f.name == name)
    }
    /// The composition containing this actual borrowed layer, without ID guessing.
    pub fn containing_composition(&self, layer: &Layer) -> Option<&Composition> {
        self.compositions.iter().find(|c| c.contains(layer))
    }
    /// Dimensions of the saved AV source. No composition-size approximation for
    /// missing or non-AV sources; no media is opened.
    pub fn source_dimensions_of(&self, layer: &Layer) -> Option<(u16, u16)> {
        match self.source_of(layer)? {
            AvItemRef::Composition(c) => Some((c.width?, c.height?)),
            AvItemRef::Footage(f) => Some((f.width?, f.height?)),
        }
    }
    /// Potential audio source by saved item kind; not proof of an audio stream.
    pub fn audio_source_possible(&self, layer: &Layer) -> Option<bool> {
        if matches!(
            layer.kind,
            LayerKind::Camera
                | LayerKind::Light
                | LayerKind::Null
                | LayerKind::Shape
                | LayerKind::Text
        ) {
            return Some(false);
        }
        match self.source_of(layer)? {
            AvItemRef::Composition(_) => Some(true),
            AvItemRef::Footage(f) => Some(!f.is_solid && !f.is_placeholder),
        }
    }
    pub fn audio_enabled_for(&self, layer: &Layer) -> Option<bool> {
        let enabled = layer.switches.as_ref()?.audio;
        if !enabled {
            Some(false)
        } else {
            self.audio_source_possible(layer)
        }
    }
    /// First match. Duplicate IDs remain visible in the original ordered arrays.
    pub fn item_by_id(&self, id: u32) -> Option<&Item> {
        let id = reference(id)?;
        self.items.iter().find(|n| n.id == id)
    }
    pub fn composition_by_id(&self, id: u32) -> Option<&Composition> {
        let id = reference(id)?;
        self.compositions.iter().find(|n| n.id == id)
    }
    pub fn footage_by_id(&self, id: u32) -> Option<&Footage> {
        let id = reference(id)?;
        self.footage.iter().find(|n| n.id == id)
    }
    pub fn av_item_by_id(&self, id: u32) -> Option<AvItemRef<'_>> {
        self.composition_by_id(id)
            .map(AvItemRef::Composition)
            .or_else(|| self.footage_by_id(id).map(AvItemRef::Footage))
    }
    pub fn layers(&self) -> impl Iterator<Item = &Layer> {
        self.compositions.iter().flat_map(|c| &c.layers)
    }
    pub fn layer_by_id(&self, id: u32) -> Option<&Layer> {
        let id = reference(id)?;
        self.layers().find(|l| l.id == id)
    }
    /// Direct item children in saved panel order. ID zero selects the virtual root.
    pub fn item_children(&self, parent_id: u32) -> impl Iterator<Item = &Item> {
        self.items.iter().filter(move |n| n.parent_id == parent_id)
    }
    pub fn root_folder(&self) -> Option<&Item> {
        self.items.iter().find(|n| n.kind == ItemKind::Folder)
    }
    /// AV-item reference. Camera/light source IDs have other meanings and are excluded.
    pub fn source_of(&self, layer: &Layer) -> Option<AvItemRef<'_>> {
        if matches!(layer.kind, LayerKind::Camera | LayerKind::Light) {
            return None;
        }
        self.av_item_by_id(layer.source_id?)
    }
    pub fn alternate_source_of(&self, layer: &Layer) -> Option<AvItemRef<'_>> {
        self.av_item_by_id(layer.alternate_source_id?)
    }
}
impl Composition {
    pub fn layer_by_name(&self, name: &str) -> Option<&Layer> {
        self.layers.iter().find(|l| l.name == name)
    }
    /// Saved audio switch aggregation; an unknown layer prevents a known false.
    pub fn any_audio_enabled(&self) -> Option<bool> {
        if self
            .layers
            .iter()
            .any(|l| l.switches.as_ref().is_some_and(|s| s.audio))
        {
            return Some(true);
        }
        self.layers
            .iter()
            .all(|l| l.switches.is_some())
            .then_some(false)
    }
    pub fn matte_source_of(&self, layer: &Layer) -> Option<&Layer> {
        if !matches!(layer.track_matte_mode, Some(1..=4)) {
            return None;
        }
        self.explicit_matte_source_of(layer)
            .or_else(|| self.legacy_matte_source_of(layer))
    }
    pub fn is_matte_source(&self, layer: &Layer) -> Option<bool> {
        if !self.contains(layer) {
            return None;
        }
        if self.layers.iter().any(|n| {
            !std::ptr::eq(n, layer)
                && self
                    .matte_source_of(n)
                    .is_some_and(|s| std::ptr::eq(s, layer))
        }) {
            return Some(true);
        }
        self.layers
            .iter()
            .filter(|n| !std::ptr::eq(*n, layer))
            .all(|n| {
                n.track_matte_mode == Some(0)
                    || (matches!(n.track_matte_mode, Some(1..=4))
                        && self.matte_source_of(n).is_some())
            })
            .then_some(false)
    }
    pub fn layer_by_id(&self, id: u32) -> Option<&Layer> {
        let id = reference(id)?;
        self.layers.iter().find(|l| l.id == id)
    }
    pub fn layers_of_kind(&self, kind: LayerKind) -> impl Iterator<Item = &Layer> {
        self.layers.iter().filter(move |l| l.kind == kind)
    }
    fn contains(&self, layer: &Layer) -> bool {
        self.layers.iter().any(|l| std::ptr::eq(l, layer))
    }
    /// One-hop lookup in this composition; cycles are not recursively followed.
    pub fn parent_of(&self, layer: &Layer) -> Option<&Layer> {
        if !self.contains(layer) {
            return None;
        }
        self.layer_by_id(layer.parent_id?)
    }
    pub fn explicit_matte_source_of(&self, layer: &Layer) -> Option<&Layer> {
        if !self.contains(layer) {
            return None;
        }
        self.layer_by_id(layer.track_matte_layer_id?)
    }
    /// Layer-above convention only when the explicit ID field is absent and a known
    /// legacy matte mode is stored. A saved explicit zero never triggers this fallback.
    pub fn legacy_matte_source_of(&self, layer: &Layer) -> Option<&Layer> {
        if layer.track_matte_layer_id.is_some() || !matches!(layer.track_matte_mode, Some(1..=4)) {
            return None;
        }
        let i = self
            .layers
            .iter()
            .position(|n| std::ptr::eq(n, layer))?
            .checked_sub(1)?;
        self.layers.get(i)
    }
    pub fn light_source_of(&self, layer: &Layer) -> Option<&Layer> {
        if layer.kind != LayerKind::Light || !self.contains(layer) {
            return None;
        }
        self.layer_by_id(layer.source_id?)
    }
    /// First visible camera in saved timeline order. This is not a rendered-time query.
    pub fn active_camera(&self) -> Option<&Layer> {
        self.layers_of_kind(LayerKind::Camera)
            .find(|l| l.switches.as_ref().is_some_and(|s| s.visible))
    }
}
impl Layer {
    /// Direct hierarchy walk. Repeated names select the first direct child.
    pub fn node_by_path(&self, path: &[&str]) -> Option<&crate::PropertyNode> {
        let (head, rest) = path.split_first()?;
        let mut node = self.properties.iter().find(|n| n.match_name == *head)?;
        for name in rest {
            node = node.children.iter().find(|n| n.match_name == *name)?;
        }
        Some(node)
    }
    pub fn property_by_path(&self, path: &[&str]) -> Option<&crate::Property> {
        self.node_by_path(path)?.property.as_ref()
    }
    /// Actual parent in this layer's tree. Root entries and foreign nodes return None.
    pub fn parent_node_of(&self, node: &crate::PropertyNode) -> Option<&crate::PropertyNode> {
        fn parent<'a>(
            nodes: &'a [crate::PropertyNode],
            target: &crate::PropertyNode,
        ) -> Option<&'a crate::PropertyNode> {
            nodes.iter().find_map(|n| {
                if n.children.iter().any(|c| std::ptr::eq(c, target)) {
                    Some(n)
                } else {
                    parent(&n.children, target)
                }
            })
        }
        parent(&self.properties, node)
    }
    /// Go's decoded-keyframe heuristic, not expression evaluation or an AE host query.
    pub fn time_remap_enabled(&self) -> bool {
        self.property("ADBE Time Remapping")
            .is_some_and(|p| !p.keyframes.is_empty())
    }
    /// Presence of a completely decoded media-replacement slot, including saved zero.
    pub fn has_alternate_source_slot(&self) -> bool {
        self.alternate_source_id.is_some()
    }
}
impl Footage {
    pub fn main_source(&self) -> FootageSourceRef<'_> {
        if self.is_placeholder {
            FootageSourceRef::Placeholder
        } else if self.is_solid {
            FootageSourceRef::Solid {
                color: self.solid_color.as_ref(),
            }
        } else {
            FootageSourceRef::File {
                path: self.path.as_deref(),
                sequence_members: &self.sequence_members,
            }
        }
    }
    pub fn has_audio(&self) -> Option<bool> {
        if self.is_solid || self.is_placeholder {
            return Some(false);
        }
        Some(self.audio_sample_rate?.value? > 0.0)
    }
    /// Saved flag only; never tests the filesystem.
    pub fn file_missing_at_save(&self) -> Option<bool> {
        if self.is_solid || self.is_placeholder {
            Some(false)
        } else {
            self.missing_at_save
        }
    }
}
