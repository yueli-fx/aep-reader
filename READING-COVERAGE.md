# Reading capabilities

This describes the data exposed by `aep-reader`, its public accessors and decoding
limits. Supported fields can be used independently of any consuming application.
[VALIDATION.md](VALIDATION.md) records the tested behavior.

## Decoded data

| Area | Representation and implementation | Boundary |
|---|---|---|
| Binary container | Borrowed `Document::root` chunk tree and trailing bytes; [rifx.rs](src/rifx.rs) | RIFX/Egg! `.aep` only; unknown chunks retain their bytes |
| Project | `Project` settings, bit depth, item IDs/hierarchy/order, effect identities; [decode.rs](src/decode.rs), [metadata.rs](src/metadata.rs) | Saved metadata; profiles and OCIO files remain path references |
| Compositions | Dimensions, frame rate, timing fractions, work area, renderer identity and motion blur | Saved layout only; no renderer implementation |
| Layers | Kind, IDs, timing, stretch, switches, orientation, source/parent/matte and alternate-source metadata | Missing or truncated optional fields remain absent |
| Property tree | Ordered `PropertyNode` entries, duplicate match names, display names/provenance and source spans | Group decode status does not establish child completeness |
| Numeric properties | `Property` static values, flags, types, ranges and separated dimensions | Storage dimensions and values retain source conventions |
| Animation | Original ticks, seconds, values, interpolation codes, easing and spatial tangents | Orientation interpolation semantics remain partial; no animation evaluator |
| Expressions | Saved source and enabled state | Source is never executed |
| Effects | Project catalog and instance-local parameter definitions, known numeric/default/range layouts and enum labels; [effects.rs](src/effects.rs) | Some integer/color/unknown definition bodies remain partial; definitions do not establish effective instance values |
| Masks and paths | Mask metadata, normalized Bezier vertices/control points, bounds, static paths and path keyframes; [geometry.rs](src/geometry.rs) | `layer_vertices()` restores coordinates; mismatched tables retain unmatched snapshots without assigning times |
| Markers and guides | Texts, times, labels and guide positions; [metadata.rs](src/metadata.rs) | Marker duration uses an explicitly unverified compatibility layout |
| Text | Static content, fonts/axes, style and paragraph fields/ranges, bounds and kerning; [text.rs](src/text.rs) | UTF-16 ranges require valid stored counts; source-text animation remains unverified |
| Shapes | Borrowed `ShapeView` for groups, primitives, transforms, fills/strokes and operators; [shape.rs](src/shape.rs) | Exposes stored property streams; no operator execution |
| Gradients | Every color/alpha XML snapshot, source span and validated 64-byte time record; [gradient.rs](src/gradient.rs) | Snapshot counts must match; easing bytes and interpolation evaluation remain unverified |
| Essential Graphics | CIF3 template/controller metadata, UUID, order and raw type; [panels.rs](src/panels.rs) | Bindings, older CIF generations and .mogrt export are unsupported |
| Render queue | Saved item/module settings, timing, output paths and known format options; [queue.rs](src/queue.rs) | Unknown codec payloads stay raw; no render or path expansion |
| Footage | Dimensions, saved paths/sequence descriptors, solid/placeholder metadata and audio information | No media decoding, sequence expansion or filesystem queries |

## Property queries

`PropertyNode::enabled` exposes saved `tdsb` bit 0, including the direct `tdgp`
inside an effect wrapper. Missing or short flag records return `None`; the reader
does not infer inherited or layer-wide enable state.

`Project::effect_definitions` exposes the saved `EfdG/EfDf` catalog, including
source spans. `effect_definition(match_name)` returns only a unique exact-name
match; missing or duplicate names return `None`, while duplicate entries remain
in the array. This catalog is separate from `PropertyNode::effect_parameters`,
which retains only an instance's own `parT`. Empty instance tables stay empty.
Neither query merges scopes, creates missing parameter properties, nor evaluates
an elided parameter's effective value. In particular, raw enum default fields
need not be usable as one-based option values.

`property_unit()` and `PropertyNode::units()` use the compiled unit catalog in
[units.rs](src/units.rs). Unknown angle controls can fall back to degrees. Unit
labels do not rescale values.

`Property::default_value()` prefers file-contained effect defaults. Otherwise it
can expose a convention tagged with `DefaultOrigin`: fixed transform values,
composition-center position, layer-origin anchor for null/text/shape layers or
composition-size anchor fallback for other layers. The latter is an approximation,
not a source-footage-size calculation. Stored values are never filled in or
replaced. These queries are implemented in [defaults.rs](src/defaults.rs).

`is_modified()` returns `Option<bool>`. Known animation or a nonempty expression
indicates modification. Numeric comparisons require a known baseline and finite
values. Groups propagate a known modification; an unknown child prevents an
unchanged result unless another child proves modification. Indexed groups use
child presence. This is a reader-derived query, not an authoritative AE UI state.

Name provenance distinguishes match-name fallback, `tdsn` instance names and
`fnam` effect names. Embedded lengths are checked and the `-_0_/-` default-name
marker is suppressed. Explicit empty names remain explicit. Invalid UTF-8 is
decoded lossily; the original source bytes remain available.

## Borrowed references

[views.rs](src/views.rs) provides item/composition/footage ID lookups, ordered
children/layers, parent/source/alternate-source/matte/light queries, the first
visible camera and `FootageSourceRef` views. References resolve one hop without
following cycles. Duplicate IDs retain their arrays; lookups select the first.
Zero/sentinel and dangling IDs return `None`.

Parent, matte and light queries require the actual layer borrowed from the
composition. Camera/light source IDs are excluded from AV-item source lookup.
Legacy matte fallback requires an absent explicit field and a known stored mode;
a present zero is not converted into a reference. The active-camera query uses
saved order and visibility, not time evaluation. Time-remap-enabled uses decoded
keyframe presence. Footage audio and saved missing-file queries preserve missing
evidence and never open media paths.

Name lookups select the first saved match. `containing_composition()` and
`parent_node_of()` require actual borrowed objects, preserving reference locality.
`node_by_path()` and `property_by_path()` walk direct hierarchy steps, preserving
duplicate-name order. Source dimensions use saved AV items without fallback guesses.
Audio queries distinguish enabled switches/potential sources from actual streams.
Matte-source aggregation preserves unknown modes. Raw chunks offer saved-order
`walk()`, repeated ID/form queries and checked unsigned reads; the query traversal
stack is outside parser allocation budgets.

## Payload, range and frame views

`parse_text_document[_with_limits]()` reads stacked `btds` sub-LISTs without an AEP
container. `parse_gradient_xml[_with_limits]()` decodes the supported XML subset.
Both enforce input, depth and allocation limits and never evaluate their contents.

Style and paragraph `TextRange` values count UTF-16 code units, including stored CR
terminators. Missing, fractional, negative or out-of-bounds lengths leave ranges
absent and prevent inferred later offsets. `slice()` returns `None` for a boundary
inside a surrogate pair. Text style queries resolve font-table names and known
caps/baseline codes, returning `None` for missing or unknown evidence.

[time.rs](src/time.rs) exposes checked `FrameRate` conversions, composition display
start/work-area frames, source-relative layer frames and keyframe/marker frames.
Conversions round ties away from zero and reject invalid rates/non-finite or
overflowing results. They do not apply layer stretch, time remapping or rendering.

## Stored value conventions

Numeric values and enum codes retain their source representation. Storage can
contain three dimensions on a 2D layer. Ease influence and layer stretch use
ratios. Text retains paragraph-ending CR characters. Path coordinates remain
bbox-normalized. A disk segment contains its anchor, its outgoing control point
and the next vertex's incoming control point; the reader reassembles controls per
vertex. `bounds_vertices()` denormalizes with each snapshot's bounds and returns
absolute control points, which adapters may convert to anchor-relative vectors.
Vector-path bounds typically use pixels. Masks on source-backed layers can use
source-normalized bounds: consumers must apply source dimensions, while shape-layer
masks use pixel bounds. The historical `layer_vertices()` name is a compatibility
alias and does not imply that all bounds are already in pixels.
Consumers should translate conventions to their own model.

Marker duration currently reads `NmHd` offset `0x08` divided by 600, with
`duration_layout_verified = false` and a diagnostic. Render template names use
Latin-1 decoding, which does not fully implement Windows-1252. Render settings
retain `0xFFFF` current-settings values; `resolved_time_span()` requires valid
ratios or the matching composition and returns `None` for missing references,
unknown source codes and zero divisors. Known output format variants are Cineon,
JPEG, OpenEXR, Targa, TIFF and PNG. Other codec settings remain raw.

## Incomplete and out-of-scope operations

Further format evidence is needed for gradient easing/evaluation and source-text animation,
unknown effect/codec payloads, marker duration layouts and
additional AE versions. Raw access preserves inputs without claiming semantic
support. Review `DecodeStatus` and `project.diagnostics` before importing fields.

The package does not write or generate AEPs, render frames, execute expressions
or plugins, export .mogrt files or relink media. Application adapters own their
own effect mappings, timing/color conversions and asset resolution.
