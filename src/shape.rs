//! Borrowed typed views over the ordered property tree; no second graph allocation.
use crate::{Layer, Property, PropertyNode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum ShapeKind {
    Root,
    Group,
    Contents,
    Transform,
    Rectangle,
    Ellipse,
    Star,
    Path,
    Fill,
    Stroke,
    GradientFill,
    GradientStroke,
    TrimPaths,
    Repeater,
    RoundCorners,
    OffsetPaths,
    MergePaths,
    ZigZag,
    PuckerBloat,
    Twist,
    WigglePaths,
    WiggleTransform,
    Unknown,
}
impl ShapeKind {
    pub fn from_match_name(name: &str) -> Self {
        match name {
            "ADBE Root Vectors Group" => Self::Root,
            "ADBE Vector Group" => Self::Group,
            "ADBE Vectors Group" => Self::Contents,
            "ADBE Vector Transform Group" => Self::Transform,
            "ADBE Vector Shape - Rect" => Self::Rectangle,
            "ADBE Vector Shape - Ellipse" => Self::Ellipse,
            "ADBE Vector Shape - Star" => Self::Star,
            "ADBE Vector Shape - Group" => Self::Path,
            "ADBE Vector Graphic - Fill" => Self::Fill,
            "ADBE Vector Graphic - Stroke" => Self::Stroke,
            "ADBE Vector Graphic - G-Fill" => Self::GradientFill,
            "ADBE Vector Graphic - G-Stroke" => Self::GradientStroke,
            "ADBE Vector Filter - Trim" => Self::TrimPaths,
            "ADBE Vector Filter - Repeater" => Self::Repeater,
            "ADBE Vector Filter - RC" => Self::RoundCorners,
            "ADBE Vector Filter - Offset" => Self::OffsetPaths,
            "ADBE Vector Filter - Merge" => Self::MergePaths,
            "ADBE Vector Filter - Zigzag" => Self::ZigZag,
            "ADBE Vector Filter - PB" => Self::PuckerBloat,
            "ADBE Vector Filter - Twist" => Self::Twist,
            "ADBE Vector Filter - Roughen" => Self::WigglePaths,
            "ADBE Vector Filter - Wiggler" => Self::WiggleTransform,
            _ => Self::Unknown,
        }
    }
}

/// A file-order view. Traversal preserves unknown nodes and container boundaries.
#[derive(Debug, Clone, Copy)]
pub struct ShapeView<'a> {
    pub node: &'a PropertyNode,
}
impl<'a> ShapeView<'a> {
    pub fn kind(self) -> ShapeKind {
        ShapeKind::from_match_name(&self.node.match_name)
    }
    pub fn children(self) -> impl Iterator<Item = Self> + 'a {
        self.node.children.iter().map(|node| Self { node })
    }
    /// Direct property only; never searches a sibling or nested shape with the same name.
    pub fn property(self, match_name: &str) -> Option<&'a Property> {
        self.node
            .children
            .iter()
            .find(|n| n.match_name == match_name)?
            .property
            .as_ref()
    }
    pub fn size(self) -> Option<&'a Property> {
        self.property(match self.kind() {
            ShapeKind::Rectangle => "ADBE Vector Rect Size",
            ShapeKind::Ellipse => "ADBE Vector Ellipse Size",
            _ => return None,
        })
    }
    pub fn position(self) -> Option<&'a Property> {
        self.property(match self.kind() {
            ShapeKind::Rectangle => "ADBE Vector Rect Position",
            ShapeKind::Ellipse => "ADBE Vector Ellipse Position",
            ShapeKind::Star => "ADBE Vector Star Position",
            ShapeKind::Transform => "ADBE Vector Position",
            _ => return None,
        })
    }
    pub fn opacity(self) -> Option<&'a Property> {
        self.property(match self.kind() {
            ShapeKind::Fill => "ADBE Vector Fill Opacity",
            ShapeKind::Stroke => "ADBE Vector Stroke Opacity",
            ShapeKind::GradientFill | ShapeKind::GradientStroke => "ADBE Vector Grad Opacity",
            ShapeKind::Transform => "ADBE Vector Group Opacity",
            _ => return None,
        })
    }
    pub fn color(self) -> Option<&'a Property> {
        self.property(match self.kind() {
            ShapeKind::Fill => "ADBE Vector Fill Color",
            ShapeKind::Stroke => "ADBE Vector Stroke Color",
            ShapeKind::GradientFill | ShapeKind::GradientStroke => "ADBE Vector Grad Colors",
            _ => return None,
        })
    }
    pub fn path(self) -> Option<&'a Property> {
        (self.kind() == ShapeKind::Path)
            .then(|| self.property("ADBE Vector Shape"))
            .flatten()
    }
}
impl Layer {
    pub fn shapes(&self) -> Option<ShapeView<'_>> {
        self.node("ADBE Root Vectors Group")
            .map(|node| ShapeView { node })
    }
}
