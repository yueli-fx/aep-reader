//! Coordinate views over stored path data. No geometry or animation evaluation.
use crate::{BezierPath, Number, TextRange, Vertex};

impl BezierPath {
    /// Denormalize all three stored control points with this snapshot's bounds.
    /// These are absolute points in the bounds' coordinate space, not relative
    /// tangents. Vector paths use pixels; masks on footage/solid/precomp layers
    /// can use source-normalized coordinates and need consumer-side source sizing.
    /// Missing, reversed or non-finite bounds/points return None.
    pub fn bounds_vertices(&self) -> Option<impl Iterator<Item = Vertex> + '_> {
        let [min_x, min_y, max_x, max_y] = self.bounds?.map(|n| n.value);
        let (min_x, min_y, max_x, max_y) = (min_x?, min_y?, max_x?, max_y?);
        let width = max_x - min_x;
        let height = max_y - min_y;
        if ![min_x, min_y, max_x, max_y, width, height]
            .iter()
            .all(|n| n.is_finite())
            || width < 0.0
            || height < 0.0
        {
            return None;
        }
        let xy = move |p: [Number; 2]| -> Option<[Number; 2]> {
            let [x, y] = p.map(|n| n.value);
            let (x, y) = (min_x + x? * width, min_y + y? * height);
            (x.is_finite() && y.is_finite()).then_some([x.into(), y.into()])
        };
        if self.vertices.iter().any(|v| {
            xy(v.anchor).is_none() || xy(v.in_tangent).is_none() || xy(v.out_tangent).is_none()
        }) {
            return None;
        }
        Some(self.vertices.iter().map(move |v| Vertex {
            anchor: xy(v.anchor).unwrap_or(v.anchor),
            in_tangent: xy(v.in_tangent).unwrap_or(v.in_tangent),
            out_tangent: xy(v.out_tangent).unwrap_or(v.out_tangent),
        }))
    }

    /// Compatibility alias for [`Self::bounds_vertices`]. Despite the historical
    /// name, source-normalized mask bounds do not automatically become pixels.
    pub fn layer_vertices(&self) -> Option<impl Iterator<Item = Vertex> + '_> {
        self.bounds_vertices()
    }
}
impl TextRange {
    /// Borrow text covered by this UTF-16 range. A boundary inside a surrogate
    /// pair, arithmetic overflow or a range outside this string returns None.
    pub fn slice(self, text: &str) -> Option<&str> {
        let end_units = self.utf16_start.checked_add(self.utf16_length)?;
        let mut units = 0usize;
        let mut start = None;
        let mut end = None;
        for (offset, character) in text.char_indices() {
            if units == self.utf16_start {
                start = Some(offset);
            }
            if units == end_units {
                end = Some(offset);
            }
            units = units.checked_add(character.len_utf16())?;
        }
        if units == self.utf16_start {
            start = Some(text.len());
        }
        if units == end_units {
            end = Some(text.len());
        }
        text.get(start?..end?)
    }
}
