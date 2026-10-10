//! Format-neutral facts. IDs, ordered property nodes, and raw source spans are retained.

#[derive(Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Project {
    pub settings: ProjectSettings,
    pub effect_names: Vec<String>,
    /// Saved EfdG/EfDf catalog, separate from each layer instance's own parT.
    pub effect_definitions: Vec<EffectDefinition>,
    pub bits_per_channel: Option<u8>,
    pub color_depth_code: Option<u8>,
    pub items: Vec<Item>,
    pub compositions: Vec<Composition>,
    pub footage: Vec<Footage>,
    pub render_queue: Option<RenderQueue>,
    pub diagnostics: Vec<Diagnostic>,
}
/// A project-level effect definition. It does not supply an instance's saved values.
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct EffectDefinition {
    pub match_name: String,
    pub parameters: Vec<EffectParameterDefinition>,
    pub source: RawSpan,
}
impl Project {
    /// Find a unique project-catalog entry by exact match name.
    /// Missing or duplicate entries return None. No instance table is overwritten,
    /// and no absent parameter is materialized or assigned a default value.
    pub fn effect_definition(&self, match_name: &str) -> Option<&EffectDefinition> {
        let mut matches = self
            .effect_definitions
            .iter()
            .filter(|d| d.match_name == match_name);
        let first = matches.next()?;
        matches.next().is_none().then_some(first)
    }
}
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Item {
    pub id: u32,
    pub parent_id: u32,
    pub order: usize,
    pub name: String,
    pub comment: String,
    pub kind: ItemKind,
    pub raw_type: u16,
    pub label: Option<u8>,
    pub source: RawSpan,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum ItemKind {
    Folder,
    Composition,
    Footage,
    Unknown,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Composition {
    pub id: u32,
    pub name: String,
    pub width: Option<u16>,
    pub height: Option<u16>,
    pub frame_rate: Option<f64>,
    /// Original 16.16 frame-rate encoding, not rounded decimal JSON.
    pub frame_rate_fixed: Option<u32>,
    pub pixel_aspect: Option<Ratio>,
    pub duration_seconds: Option<f64>,
    pub display_start: Option<Ratio>,
    pub work_area_start: Option<Ratio>,
    /// None when the source uses the 0xFFFFFFFF full-duration sentinel.
    pub work_area_end: Option<Ratio>,
    pub work_area_uses_duration: bool,
    pub background_rgb: Option<[u8; 3]>,
    pub keyframe_ticks_per_second: u32,
    pub shutter_angle: Option<u16>,
    pub shutter_phase: Option<i32>,
    pub renderer: Option<String>,
    pub resolution_factor: Option<[u16; 2]>,
    pub motion_blur_adaptive_sample_limit: Option<i32>,
    pub motion_blur_samples_per_frame: Option<i32>,
    pub markers: Vec<Marker>,
    pub guides: Vec<Guide>,
    pub essential_graphics: Option<EssentialGraphics>,
    pub layers: Vec<Layer>,
    pub source: RawSpan,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Layer {
    pub id: u32,
    pub index: usize,
    pub name: String,
    pub comment: String,
    pub kind: LayerKind,
    pub source_id: Option<u32>,
    pub parent_id: Option<u32>,
    /// Signed fractions exactly as stored. Target time-space conversion is an adapter concern.
    pub start_time: Option<Ratio>,
    pub source_in_point: Option<Ratio>,
    pub source_out_point: Option<Ratio>,
    /// Ratio (1 = normal), not AE's percentage UI.
    pub stretch: Option<Ratio>,
    pub label: Option<u8>,
    pub quality: Option<u16>,
    pub blend_mode: Option<u8>,
    pub track_matte_mode: Option<u8>,
    /// AE 23+ explicit source ID. Older files use the layer-above convention.
    pub track_matte_layer_id: Option<u32>,
    pub light_kind: Option<u32>,
    pub preserve_transparency: Option<bool>,
    pub alternate_source_id: Option<u32>,
    pub auto_orient: Option<AutoOrient>,
    pub markers: Vec<Marker>,
    pub switches: Option<Switches>,
    pub properties: Vec<PropertyNode>,
    pub text_source: Option<TextDocument>,
    pub source: RawSpan,
}
impl Layer {
    /// First occurrence only; iterate the ordered tree when names are repeated.
    pub fn node(&self, match_name: &str) -> Option<&PropertyNode> {
        self.properties.iter().find_map(|n| n.find(match_name))
    }
    pub fn property(&self, match_name: &str) -> Option<&Property> {
        self.node(match_name)?.property.as_ref()
    }
    pub fn effects(&self) -> &[PropertyNode] {
        self.node("ADBE Effect Parade")
            .map(|n| n.children.as_slice())
            .unwrap_or_default()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum LayerKind {
    Av,
    Null,
    Adjustment,
    Text,
    Shape,
    Camera,
    Light,
    Model3d,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum AutoOrient {
    None,
    AlongPath,
    CameraOrPointOfInterest,
    CharactersTowardCamera,
}
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Switches {
    pub visible: bool,
    pub audio: bool,
    pub effects: bool,
    pub motion_blur: bool,
    pub frame_blending: bool,
    pub locked: bool,
    pub shy: bool,
    pub collapse_transformations: bool,
    pub adjustment: bool,
    pub three_d: bool,
    pub solo: bool,
    pub null: bool,
    pub markers_locked: bool,
    pub guide: bool,
    pub bicubic: bool,
    pub pixel_motion: bool,
    /// Original bytes retain switches not yet interpreted.
    pub raw: [u8; 3],
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Footage {
    pub id: u32,
    pub name: String,
    /// Stored reference only; the reader never opens it.
    pub path: Option<String>,
    pub width: Option<u16>,
    pub height: Option<u16>,
    pub format_tag: Option<String>,
    pub solid_color: Option<[Number; 3]>,
    pub is_placeholder: bool,
    pub is_solid: bool,
    /// Flag saved by AE, not a current filesystem existence check.
    pub missing_at_save: Option<bool>,
    pub audio_sample_rate: Option<Number>,
    pub start_frame: Option<u32>,
    pub end_frame: Option<u32>,
    pub sequence_members: Vec<String>,
    pub source: RawSpan,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct PropertyNode {
    /// Saved tdsb bit 0 on this node (or an effect's direct tdgp); missing stays unknown.
    pub enabled: Option<bool>,
    pub match_name: String,
    pub name: String,
    pub name_source: NameSource,
    pub kind: NodeKind,
    pub decode_status: DecodeStatus,
    pub children: Vec<PropertyNode>,
    pub property: Option<Property>,
    pub mask: Option<MaskInfo>,
    pub index: usize,
    pub depth: usize,
    /// Definitions in this effect's own parT; no external catalog is consulted.
    pub effect_parameters: Vec<EffectParameterDefinition>,
    pub source: RawSpan,
}
impl PropertyNode {
    pub fn find(&self, match_name: &str) -> Option<&Self> {
        if self.match_name == match_name {
            return Some(self);
        }
        self.children.iter().find_map(|n| n.find(match_name))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum NodeKind {
    Group,
    Property,
    Opaque,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum DecodeStatus {
    Decoded,
    Partial,
    Unsupported,
    Failed,
}

#[derive(Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Property {
    pub storage_dimensions: Option<u8>,
    pub spatial: bool,
    pub static_value: Option<Value>,
    pub keyframes: Vec<Keyframe>,
    pub expression: Option<String>,
    pub expression_enabled: Option<bool>,
    pub layer_reference_id: Option<u32>,
    pub metadata: PropertyMetadata,
    pub effect_definition: Option<EffectParameterDefinition>,
    /// A reader convention or heuristic; never replaces a stored static value.
    pub inferred_default: Option<InferredDefault>,
    /// File-order XML snapshots. Timing association is deliberately not inferred.
    pub gradient_snapshots: Vec<GradientSnapshot>,
    /// Path snapshots that cannot be associated with a static value or time table.
    pub unmatched_path_snapshots: Vec<PathSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum NameSource {
    MatchName,
    InstanceName,
    EffectInstanceName,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum DefaultOrigin {
    EffectDefinition,
    FixedConvention,
    CompositionCenter,
    LayerOrigin,
    /// Go compatibility approximation for AV/camera/light anchors; source size is not used.
    CompositionSizeFallback,
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct InferredDefault {
    pub origin: DefaultOrigin,
    pub(crate) values: [Number; 3],
    pub(crate) dimensions: u8,
}
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DefaultValue<'a> {
    pub numbers: &'a [Number],
    pub origin: DefaultOrigin,
}
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum Value {
    Numbers(Vec<Number>),
    Path(BezierPath),
}
impl Value {
    pub fn numbers(&self) -> Option<&[Number]> {
        match self {
            Self::Numbers(n) => Some(n),
            Self::Path(_) => None,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Number {
    pub value: Option<f64>,
    pub non_finite: Option<NonFinite>,
}
impl From<f64> for Number {
    fn from(v: f64) -> Self {
        Self {
            value: v.is_finite().then_some(v),
            non_finite: if v.is_finite() {
                None
            } else if v.is_nan() {
                Some(NonFinite::NaN)
            } else if v.is_sign_positive() {
                Some(NonFinite::PositiveInfinity)
            } else {
                Some(NonFinite::NegativeInfinity)
            },
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum NonFinite {
    NaN,
    PositiveInfinity,
    NegativeInfinity,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Keyframe {
    pub time_ticks: u32,
    pub time_seconds: f64,
    pub value: Option<Value>,
    pub in_interpolation: u8,
    pub out_interpolation: u8,
    pub in_ease: Vec<TemporalEase>,
    pub out_ease: Vec<TemporalEase>,
    pub in_spatial_tangent: Vec<Number>,
    pub out_spatial_tangent: Vec<Number>,
    pub raw_flags: [u8; 2],
}
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TemporalEase {
    pub speed: Number,
    /// Ratio units, e.g. 0.3333, not percentage.
    pub influence: Number,
}
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct BezierPath {
    pub closed: Option<bool>,
    /// Stored [min_x, min_y, max_x, max_y]. Vector paths typically use layer pixels;
    /// masks on source-backed layers can use source-normalized bounds.
    pub bounds: Option<[Number; 4]>,
    /// Stored bbox-normalized anchors and absolute control points, not tangent vectors.
    /// Use layer_vertices() for coordinates scaled by the stored bounds.
    pub vertices: Vec<Vertex>,
}
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct PathSnapshot {
    pub path: Option<BezierPath>,
    pub source: RawSpan,
}
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Vertex {
    pub anchor: [Number; 2],
    /// Incoming control point for this vertex, reassembled from the prior segment.
    pub in_tangent: [Number; 2],
    /// Outgoing control point for this vertex. Both controls are absolute points
    /// in the path's normalized space, not anchor-relative vectors.
    pub out_tangent: [Number; 2],
}
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MaskInfo {
    pub inverted: Option<bool>,
    pub locked: Option<bool>,
    pub mode: Option<u32>,
    pub index: Option<u32>,
    pub color: Option<[u8; 3]>,
    pub motion_blur: Option<u8>,
    pub feather_falloff: Option<u8>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RawSpan {
    pub offset: usize,
    pub length: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Ratio {
    pub numerator: i64,
    pub denominator: u32,
}
impl Ratio {
    pub fn value(self) -> Option<f64> {
        (self.denominator != 0).then(|| self.numerator as f64 / f64::from(self.denominator))
    }
}
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Diagnostic {
    pub code: &'static str,
    pub offset: usize,
    pub message: String,
}

/// Static text-engine document. Text retains AE's original CR paragraph terminators.
/// Animated source-text timing remains unverified.
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TextDocument {
    pub text: String,
    pub fonts: Vec<String>,
    /// Axis values in font-table order, decoded from 16.16 values. No fonts are loaded.
    pub font_axes: Vec<Vec<Number>>,
    pub style_runs: Vec<TextStyleRun>,
    pub paragraphs: Vec<TextParagraph>,
    pub box_bounds: Option<[Number; 4]>,
    pub manual_kerning: Vec<Number>,
    pub kerning: Option<Number>,
    pub justification: Option<Number>,
    pub decode_status: DecodeStatus,
}
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TextStyleRun {
    /// Checked UTF-16 code-unit range, including stored paragraph terminators.
    pub range: Option<TextRange>,
    pub font_index: Option<Number>,
    pub font_size: Option<Number>,
    pub tracking: Option<Number>,
    pub leading: Option<Number>,
    pub fill_argb: Option<[Number; 4]>,
    pub stroke_argb: Option<[Number; 4]>,
    pub faux_bold: Option<bool>,
    pub faux_italic: Option<bool>,
    pub auto_leading: Option<bool>,
    pub horizontal_scale: Option<Number>,
    pub vertical_scale: Option<Number>,
    pub baseline_shift: Option<Number>,
    pub tsume: Option<Number>,
    pub apply_stroke: Option<bool>,
    pub stroke_width: Option<Number>,
    pub auto_kern_type: Option<Number>,
    pub caps_option: Option<Number>,
    pub baseline_option: Option<Number>,
    pub no_break: Option<bool>,
    pub stroke_over_fill: Option<bool>,
    pub line_join_type: Option<Number>,
    pub digit_set: Option<Number>,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TextParagraph {
    pub range: Option<TextRange>,
    pub justification: Option<Number>,
    pub first_line_indent: Option<Number>,
    pub start_indent: Option<Number>,
    pub end_indent: Option<Number>,
    pub space_before: Option<Number>,
    pub space_after: Option<Number>,
    pub leading_type: Option<Number>,
    pub auto_hyphenate: Option<bool>,
    pub hanging_roman: Option<bool>,
    pub direction: Option<Number>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TextRange {
    pub utf16_start: usize,
    pub utf16_length: usize,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct EssentialGraphics {
    pub template_name: Option<String>,
    pub controllers: Vec<EssentialGraphicsController>,
    pub source: RawSpan,
}
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct EssentialGraphicsController {
    pub uuid: Option<String>,
    pub name: Option<String>,
    pub type_code: Option<u32>,
    pub source: RawSpan,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct GradientSnapshot {
    pub gradient: Option<Gradient>,
    /// Present only for a validated 64-byte time table with matching snapshot count.
    pub timing: Option<GradientTiming>,
    pub source: RawSpan,
}
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct GradientTiming {
    pub time_ticks: u32,
    pub time_seconds: f64,
    pub in_interpolation: u8,
    pub out_interpolation: u8,
    pub raw_flags: [u8; 2],
    /// Original complete time record, including unverified easing/cache bytes.
    pub source: RawSpan,
}
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Gradient {
    pub version: Option<String>,
    pub color_stops: Vec<GradientColorStop>,
    pub alpha_stops: Vec<GradientAlphaStop>,
}
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct GradientColorStop {
    pub offset: Number,
    pub midpoint: Number,
    pub color: [Number; 3],
}
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct GradientAlphaStop {
    pub offset: Number,
    pub midpoint: Number,
    pub alpha: Number,
}

#[derive(Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ProjectSettings {
    pub version_word: Option<u32>,
    pub revision: Option<u16>,
    pub compensate_for_scene_referred_profiles: Option<bool>,
    pub audio_sample_rate: Option<Number>,
    pub working_gamma: Option<Number>,
    pub gpu_accel_type: Option<String>,
    pub expression_engine: Option<String>,
    pub time_display_type: Option<u8>,
    pub footage_timecode_display_start_type: Option<u8>,
    pub frames_use_feet_frames: Option<bool>,
    pub timecode_default_base: Option<u16>,
    pub feet_frames_per_foot: Option<u32>,
    pub frames_count_type: Option<u8>,
    pub transparency_grid_thumbnails: Option<bool>,
    pub linear_blending: bool,
    pub linearize_working_space: bool,
    pub color_management: Option<ColorManagement>,
}
impl ProjectSettings {
    pub fn display_start_frame(&self) -> Option<u8> {
        self.frames_count_type.map(|n| n % 2)
    }
    pub fn version_string(&self) -> Option<String> {
        let w = self.version_word?;
        Some(format!(
            "{}.{}x{}",
            ((w >> 26) & 31) * 8 + ((w >> 19) & 7),
            (w >> 15) & 15,
            w & 255
        ))
    }
}
#[derive(Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ColorManagement {
    pub system: Option<Number>,
    pub lut_interpolation_method: Option<Number>,
    /// Reference only; the configuration and profiles are never opened.
    pub ocio_configuration_file: Option<String>,
    pub working_space: Option<String>,
}
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Marker {
    pub time_ticks: u32,
    pub time_seconds: f64,
    /// Go-compatible NmHd @0x08 decoding. Alternate duration layout is unverified.
    pub duration_seconds: Option<f64>,
    pub duration_layout_verified: bool,
    pub label: Option<u8>,
    pub raw_flags: Option<u8>,
    pub comment: String,
    pub chapter: String,
    pub url: String,
    pub frame_target: String,
    pub cue_point_name: String,
    pub source: RawSpan,
}
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Guide {
    pub orientation: u32,
    pub position_type: u32,
    pub position: Number,
    pub source: RawSpan,
}
#[derive(Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct PropertyMetadata {
    pub storage_type_code: Option<u8>,
    pub spatial: Option<bool>,
    pub static_flag: Option<bool>,
    pub animated: Option<bool>,
    pub color: Option<bool>,
    pub integer: Option<bool>,
    pub vector: Option<bool>,
    pub no_value: Option<bool>,
    pub can_vary_over_time: Option<bool>,
    pub locked_ratio: Option<bool>,
    pub dimensions_separated: Option<bool>,
    pub enabled: Option<bool>,
    pub separation_dimension: Option<u8>,
    pub separation_leader: bool,
    pub control_type: Option<u8>,
    pub value_type: PropertyValueType,
    pub min_value: Option<Value>,
    pub max_value: Option<Value>,
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum PropertyValueType {
    #[default]
    Unknown,
    NoValue,
    OneD,
    TwoD,
    ThreeD,
    TwoDSpatial,
    ThreeDSpatial,
    Color,
    LayerIndex,
    MaskIndex,
    Custom,
}
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct EffectParameterDefinition {
    pub match_name: String,
    pub name: String,
    pub control_type: u8,
    pub last_value: Option<Value>,
    pub default_value: Option<Value>,
    pub min_value: Option<Number>,
    pub max_value: Option<Number>,
    pub option_count: Option<i32>,
    pub enum_options: Vec<String>,
    pub decode_status: DecodeStatus,
    pub source: RawSpan,
}

/// Saved queue metadata only. Reading never starts a render or opens output paths.
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RenderQueue {
    pub items: Vec<RenderQueueItem>,
    pub source: RawSpan,
}
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RenderQueueItem {
    pub comment: Option<String>,
    pub settings: Option<RenderSettings>,
    pub output_modules: Vec<OutputModule>,
    pub source: RawSpan,
}
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct RenderSettings {
    pub composition_id: u32,
    pub status: u32,
    /// Raw saved name decoded as Latin-1 for compatibility with the Go reader.
    pub template_name: String,
    pub time_span_source: u16,
    pub time_span_start: Ratio,
    pub time_span_duration: Ratio,
    pub queue_item_notify: bool,
    pub elapsed_seconds: u32,
    pub resolution: [u16; 2],
    pub skip_existing_files: bool,
    pub field_render: u16,
    pub pulldown: u16,
    pub quality: u16,
    pub effects: u16,
    pub proxy_use: u16,
    pub motion_blur: u16,
    pub frame_blending: u16,
    pub log_type: u16,
    pub frame_rate: u16,
    pub solo_switches: u16,
    pub disk_cache: u16,
    pub guide_layers: u16,
    pub color_depth: u16,
    pub source: RawSpan,
}
impl RenderSettings {
    /// Resolve the saved span against its composition. Missing references, unknown
    /// source codes and zero denominators produce None rather than invented times.
    pub fn resolved_time_span(&self, composition: Option<&Composition>) -> Option<(f64, f64)> {
        match self.time_span_source {
            0 => {
                let c = composition.filter(|c| c.id == self.composition_id)?;
                Some((0.0, c.duration_seconds?))
            }
            1 => {
                let c = composition.filter(|c| c.id == self.composition_id)?;
                let start = c.work_area_start?.value()?;
                let end = if c.work_area_uses_duration {
                    c.duration_seconds?
                } else {
                    c.work_area_end?.value()?
                };
                Some((start, end - start))
            }
            2 | u16::MAX => Some((
                self.time_span_start.value()?,
                self.time_span_duration.value()?,
            )),
            _ => None,
        }
    }
}
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct OutputModule {
    pub template_name: Option<String>,
    pub file_template: Option<String>,
    pub full_path: Option<String>,
    pub settings: Option<OutputModuleSettings>,
    pub options: Option<OutputOptions>,
    pub format_options: Option<FormatOptions>,
    pub source: RawSpan,
}
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct OutputModuleSettings {
    pub use_comp_frame_number: bool,
    pub use_region_of_interest: bool,
    pub include_source_xmp: bool,
    pub preserve_rgb: bool,
    pub crop: bool,
    pub post_render_composition_id: u32,
    pub post_render_action: u32,
    pub channels: u8,
    pub resize_quality: u8,
    pub output_audio: u8,
    pub convert_to_linear: u8,
    pub color_space_working: u8,
    pub resize: bool,
    pub lock_aspect_ratio: bool,
    pub include_project_link: bool,
    pub crop_top: u16,
    pub crop_left: u16,
    pub crop_bottom: u16,
    pub crop_right: u16,
    pub source: RawSpan,
}
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct OutputOptions {
    pub video_codec: String,
    pub format_id: String,
    pub starting_number: u32,
    pub width: u16,
    pub height: u16,
    pub depth: u8,
    pub color_premultiplied: u8,
    pub audio_sample_rate: Number,
    pub audio_format: u8,
    pub audio_bit_depth: u8,
    pub audio_channels: u8,
    pub audio_enabled: bool,
    pub source: RawSpan,
}
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum FormatOptions {
    Cineon {
        black_point: u16,
        white_point: u16,
        converted_black_point: Number,
        converted_white_point: Number,
        gamma: Number,
        highlight_expansion: u16,
        logarithmic_conversion: bool,
        file_format: u8,
        bit_depth: u8,
    },
    Jpeg {
        quality: u16,
    },
    OpenExr {
        compression: u8,
        float_32: bool,
        luminance_chroma: bool,
    },
    Targa {
        bits_per_pixel: u8,
        rle_compression: bool,
    },
    Tiff {
        ibm_pc_byte_order: bool,
        lzw_compression: bool,
    },
    Png {
        width: u32,
        height: u32,
        bit_depth: u16,
        compression: u32,
    },
}
