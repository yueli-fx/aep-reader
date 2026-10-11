# Capability matrix

This is the public support contract for the reader and experimental EffectCraft
consumer. Reading a value, importing it, and reproducing an AE image are separate
claims. The reference reader is not a correctness oracle; see
[comparison boundaries](COMPARISON.md) and [provenance](PROVENANCE.md).

## Lifecycle

| State | Meaning |
|---|---|
| Verified subset / 限定范围已验收 | The exact stated contract has tests and evidence; no claim about other layouts or AE rendering. |
| Partial / 部分支持 | Implemented and checked, with the limitations in the same row. |
| Unverified / 待核验 | Data or an implementation route exists, but its semantics or coverage are not established. |
| Not connected / 未接入 | Reader data is not consumed by this adapter. |
| Conflict / 存在冲突 | Source and target semantics differ; the conflicting subset is reported and omitted. |
| Out of scope / 范围外 | Deliberately excluded from this read-only package. |

A planned feature is not supported. A feature moves to a verified subset only
when its stated boundary, malformed-input behavior and applicable original-file
or rendering checks pass. New layouts or contradictory results move it back to
partial/unverified. None of the broad rendering areas below claims full AE parity.

## Reader

| Capability | State | Available contract | Limits / evidence |
|---|---|---|---|
| Binary container and raw bytes | Verified subset | Bounded RIFX/Egg! parsing, borrowed chunk tree, source spans and trailing bytes | Public malformed/truncated/budget tests; `.aepx`/`.ffx` excluded |
| Project, comp, layer structure | Partial | Hierarchy, timing, dimensions, references, switches and saved metadata | Optional/unknown layouts remain absent; no AE evaluator |
| Ordered property tree | Verified subset | Preserves order, repeated names, container boundaries, enable flags and original spans | A decoded group does not prove every descendant is decoded |
| Numeric/path keyframes | Partial | Original ticks, values, interpolation codes, ease and tangent data | Some layouts and interpolation semantics remain unknown |
| Effect catalog | Verified subset | Separate project catalog and instance definitions; exact unique-name lookup | Definitions do not establish effective values for omitted parameters |
| Effect parameter bodies | Partial | Known numeric/range/default/enum fields | Unsupported bodies remain raw and diagnostic |
| Bezier paths and masks | Partial | Bounds, anchors, paired control points, saved mask metadata and path keys | Source-backed mask coordinates require source dimensions; mismatched snapshots retain uncertainty |
| Shape/operator views | Partial | Borrowed views over the ordered property streams | The reader does not execute operators or supply an AE renderer |
| Gradient snapshots | Partial | Independent color/alpha stops, midpoints and validated timing associations | Easing/evaluation unverified; malformed XML is bounded and diagnostic |
| Static text | Partial | Content, styles, fonts/axes, paragraphs, kerning and UTF-16 ranges | Layout and missing-field behavior are incomplete |
| SourceText content animation | Unverified | Original bytes remain accessible | No verified timed text-document sequence API |
| Markers and guides | Partial | Text, time, guide positions and saved metadata | Marker duration layout is explicitly unverified |
| Footage metadata | Partial | Saved paths, sequence descriptors, dimensions and audio metadata | No filesystem/media access or sequence expansion |
| Color settings / render queue / Essential Graphics | Partial | Saved metadata and known record layouts | Profiles, bindings and unknown codec layouts are not evaluated |
| Expressions | Verified subset | Saved source and enable state can be inspected | Never executed |
| Writing / templates / host automation | Out of scope | None | No AEP writer, AE templates, plugin or script execution |

API details: [reading coverage](READING-COVERAGE.md). Parser allocations are
bounded separately from the caller's input and allocator overhead.

## EffectCraft consumer

Target baseline: EffectCraft `813c7c4650f0d4c805da620c8d5837c9600ad2e3` (0.7.0).
The standalone adapter requires a sibling checkout; it is not a native editor
plugin. See [usage](examples/effectcraft/README.md).

| Capability | State | Implemented scope | Remaining limit / fallback |
|---|---|---|---|
| Project and layer structure | Partial | Folders, comps, solids, nulls, precomps, transforms, visibility and timing | Missing media is inactive; missing parents recover only with explicit option, losing world-space placement |
| Bit depth and audio rate | Partial | Saved 8/16/32 bpc; integer 8–192 kHz | Invalid rates retain target default with warning; this is not audio/media playback support |
| Work area / shutter / markers | Partial | Saved work area, bounded motion-blur settings, marker text/time/link fields | Unverified marker duration becomes a reported point marker |
| Numeric, color and path animation | Partial | Key values/ticks and known handles | Unknown interpolation falls back to reported linear behavior; missing values are reported |
| Separated position / time remap | Partial | Independent axes when leader enables separation; key-driven remap on timed sources | Static/expression-only remap activation unverified |
| Static text | Partial | Text/runs, basic style, paragraph and box data | Font matching, shaping and AE layout are unverified |
| Text animator channels | Partial | Opacity, planar position/scale, Z rotation/skew, fill/stroke color/opacity, stroke width, tracking and character offset; numeric keys | Non-neutral character Z and X/Y rotation, unsupported companions/channels are omitted |
| Ordered text range selectors | Partial | Up to 32 ranges; percent/index bounds, based-on/mode/shape, amount/smoothness/ease, randomize/seed | Popups static; missing index end rejects that animator; target easing/random sequences are not AE-equivalent claims |
| SourceText / wiggly / expression selectors | Not connected | None | Unsupported selector omits the whole animator; expressions never run |
| Basic shapes | Partial | Ordered groups, rectangles, ellipses, paths, fill/stroke, trim and enable state | Complex group/paint ordering needs AE comparison |
| Stroke styles / fill rule / trim mode | Partial | Static caps, joins, fill rule and both named trim modes | Animated/unknown enums retain a reported default |
| Star / polygon | Partial | Static integer 3–1024 points, direction, radii, position/rotation and roundness | Default five points reported; fractional/animated geometry omitted; contours approximate |
| Gradient fill / stroke | Partial | Static geometry, independent color/alpha stops and timed snapshots | Midpoint interpolation approximate; consistent stop topology required; animated geometry omitted |
| Repeater | Partial | Static integer 0–128 copies with bounded transform and opacity | Nested/multiple expansion guarded; fractional/animated copies omitted |
| Round / pucker-bloat / twist | Partial | Radius/amount/angle numeric keys; static twist center | Range and whole-layer expansion budgets; contour algorithms approximate |
| Zigzag | Partial | Size keys, static integer 0–32 ridges and corner/smooth mode | Fractional/animated ridges omitted; target waveform approximate |
| Merge paths | Partial | Five static named modes | Complex Boolean intersections and path order need AE comparison |
| Offset paths | Partial + Conflict | Amount keys and one copy with default source step | Source copy-step and target start-offset differ; multiple copies omitted |
| Wiggle operators | Not connected | None | Random/phase algorithms need independent mapping |
| Masks | Partial | Seven named modes, path, opacity, feather, expansion, inversion and enable state | Advanced feather/falloff/compositing still unverified |
| Gaussian blur | Partial | Bounded known parameters, keys, switches and reported definition-derived defaults | Unknown selectors or compositing options omit effect; other effects not connected |
| Blend / track matte | Partial | Named blend codes, four matte modes, old/new references and preserve transparency | Unknown blends report Normal fallback; invalid matte graphs rejected |
| Camera / lights / material | Partial | Real layer kinds, saved camera/light parameters and explicit material shadow/light switches | Active-camera timing, material coefficient units and AE 3D rendering unverified |
| Color management | Not connected | Bit depth only | Working spaces, ICC/OCIO and linear-light settings require separate pipeline mapping |
| Media relinking / native editor AEP import | Not connected | Standalone desktop file picker or AEP drop onto converter executable | Editor opens generated `.ecproj`; saved external paths are not loaded automatically |

Malformed or unsupported text animators and bounded shape operators are omitted
locally with warnings. Missing original data is not reconstructed. Keep the
`.ecproj.import.json` alongside the converted project and retain the AEP.

## Validation boundary

[Validation](VALIDATION.md) separates byte tests, native-file checks, target pixel
checks and desktop MCP checks. A converted file can still have missing content or
approximate animation. No row becomes complete merely because a corpus converts
without errors. Private source files, previews and reference API inventories are
not part of the public package.
