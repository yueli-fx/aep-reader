//! Explicitly sourced default and derived-state views. Stored values are never filled in.
use crate::{
    DefaultOrigin, DefaultValue, InferredDefault, Layer, LayerKind, NameSource, Number, Property,
    PropertyNode,
};

impl Property {
    /// File-contained effect defaults take precedence over inferred conventions.
    pub fn default_value(&self) -> Option<DefaultValue<'_>> {
        if let Some(numbers) = self
            .effect_definition
            .as_ref()
            .and_then(|d| d.default_value.as_ref())
            .and_then(|v| v.numbers())
        {
            return Some(DefaultValue {
                numbers,
                origin: DefaultOrigin::EffectDefinition,
            });
        }
        let d = self.inferred_default.as_ref()?;
        Some(DefaultValue {
            numbers: d.values.get(..usize::from(d.dimensions))?,
            origin: d.origin,
        })
    }
    /// Explicit descriptor flag when available; otherwise decoded keyframe presence.
    pub fn is_animated(&self) -> bool {
        !self.keyframes.is_empty()
            || self.gradient_snapshots.iter().any(|s| s.timing.is_some())
            || self.metadata.animated == Some(true)
    }
    /// Reader-derived comparison with a known numeric baseline, not AE's UI state.
    /// No baseline, absent values and non-finite comparisons yield None.
    pub fn is_modified(&self) -> Option<bool> {
        if self.is_animated()
            || !self.keyframes.is_empty()
            || self.expression.as_ref().is_some_and(|s| !s.is_empty())
        {
            return Some(true);
        }
        let value = self.static_value.as_ref()?.numbers()?;
        let default = self.default_value()?.numbers;
        if value.len() != default.len() {
            return Some(true);
        }
        let mut modified = false;
        for (a, b) in value.iter().zip(default) {
            let (a, b) = (a.value?, b.value?);
            if !a.is_finite() || !b.is_finite() {
                return None;
            }
            if (a - b).abs() > 1e-6 {
                modified = true;
            }
        }
        Some(modified)
    }
}
impl PropertyNode {
    pub fn has_explicit_name(&self) -> bool {
        self.name_source != NameSource::MatchName
    }
    /// Go-compatible name query. An explicitly stored identical name returns false.
    pub fn is_name_set(&self) -> bool {
        !self.name.is_empty() && self.name != self.match_name
    }
    /// Modified indexed groups are identified by child presence. Other groups use
    /// three-state aggregation, so unknown children do not masquerade as unchanged.
    pub fn is_modified(&self) -> Option<bool> {
        if let Some(p) = &self.property {
            return p.is_modified();
        }
        if self.kind != crate::NodeKind::Group {
            return None;
        }
        if matches!(
            self.match_name.as_str(),
            "ADBE Effect Parade" | "ADBE Mask Parade" | "ADBE Root Vectors Group"
        ) {
            return Some(!self.children.is_empty());
        }
        let mut known = true;
        for child in &self.children {
            match child.is_modified() {
                Some(true) => return Some(true),
                None => known = false,
                _ => {}
            }
        }
        known.then_some(false)
    }
}
fn scalar(n: f64) -> InferredDefault {
    numbers([n, 0.0, 0.0], 1, DefaultOrigin::FixedConvention)
}
fn numbers(values: [f64; 3], dimensions: u8, origin: DefaultOrigin) -> InferredDefault {
    InferredDefault {
        values: values.map(Number::from),
        dimensions,
        origin,
    }
}
fn inferred(
    name: &str,
    kind: LayerKind,
    width: Option<u16>,
    height: Option<u16>,
) -> Option<InferredDefault> {
    match name {
        // Stored layer scale and opacity are ratios, even though UI units are percent.
        "ADBE Scale" => Some(numbers([1.0; 3], 3, DefaultOrigin::FixedConvention)),
        "ADBE Orientation" => Some(numbers([0.0; 3], 3, DefaultOrigin::FixedConvention)),
        "ADBE Rotate X" | "ADBE Rotate Y" | "ADBE Rotate Z" | "ADBE Position_2" => {
            Some(scalar(0.0))
        }
        "ADBE Opacity" => Some(scalar(1.0)),
        "ADBE Envir Appear in Reflect" => Some(scalar(1.0)),
        "ADBE Anchor Point"
            if matches!(kind, LayerKind::Shape | LayerKind::Text | LayerKind::Null) =>
        {
            Some(numbers([0.0; 3], 3, DefaultOrigin::LayerOrigin))
        }
        "ADBE Anchor Point" => Some(numbers(
            [f64::from(width?) / 2.0, f64::from(height?) / 2.0, 0.0],
            3,
            DefaultOrigin::CompositionSizeFallback,
        )),
        "ADBE Position" => Some(numbers(
            [f64::from(width?) / 2.0, f64::from(height?) / 2.0, 0.0],
            3,
            DefaultOrigin::CompositionCenter,
        )),
        "ADBE Position_0" => Some(numbers(
            [f64::from(width?) / 2.0, 0.0, 0.0],
            1,
            DefaultOrigin::CompositionCenter,
        )),
        "ADBE Position_1" => Some(numbers(
            [f64::from(height?) / 2.0, 0.0, 0.0],
            1,
            DefaultOrigin::CompositionCenter,
        )),
        _ => None,
    }
}
pub(crate) fn assign(layer: &mut Layer, width: Option<u16>, height: Option<u16>) {
    fn visit(nodes: &mut [PropertyNode], kind: LayerKind, width: Option<u16>, height: Option<u16>) {
        for node in nodes {
            if node.match_name == "ADBE Effect Parade" {
                continue;
            }
            if let Some(p) = &mut node.property {
                p.inferred_default = inferred(&node.match_name, kind, width, height);
            }
            visit(&mut node.children, kind, width, height);
        }
    }
    visit(&mut layer.properties, layer.kind, width, height);
}
