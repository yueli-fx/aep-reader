# AEP → EffectCraft experimental consumer

A standalone Rust adapter and CLI using aep-reader and EffectCraft's own project
model. It writes `.ecproj` through `Project::to_file_json()`, which checks reopening
with the same `Project::from_json()` used by EffectCraft's File > Open command.
This is an actively developed integration, with a limited, tested mapping subset.

## Windows desktop use

Double-click `AEP转EffectCraft.exe` to choose an AEP, or drag one AEP onto the
executable icon. It creates `.ecproj` and `.ecproj.import.json` beside the AEP,
then shows the output path and warning count in a dialog. Open the `.ecproj`
in EffectCraft; it is the target JSON project format. Existing outputs are
preserved by choosing an `-import-N` suffix. Canceling selection creates nothing.
Conversion failures show a dialog. The converter is separate from the editor;
dropping AEP into the EffectCraft editor itself is not implemented yet.

## Run

Requires Rust 1.95+ and sibling checkouts named `aep-reader` and `effectcraft`.
EffectCraft APIs were checked at `813c7c4650f0d4c805da620c8d5837c9600ad2e3`
(0.7.0). Cargo.lock pins registry dependencies; sibling path dependencies are not
pinned by Cargo.lock. A newer EffectCraft checkout may require adapter updates.

From the aep-reader root, in PowerShell:

```powershell
$env:CARGO_TARGET_DIR = "$PWD/.task/effectcraft-local/target"
cargo run --locked --manifest-path examples/effectcraft/Cargo.toml -- input.aep output.ecproj
```

Open `output.ecproj` in EffectCraft and read `output.ecproj.import.json` for
unsupported features and fallbacks. Both output files must be new. Input is
bounded to 256 MiB by the CLI and further bounded by the reader's parser limits.
Keep the original AEP: the report references original byte offsets; unknown bytes
are not embedded in ecproj. No original sample files are distributed here.
Remove task build outputs when no longer needed; retain your converted deliverables.

## Mapping scope

| Area | Current mapping |
|---|---|
| Project | Folder hierarchy, item names/comments, composition dimensions, stored rational frame rate, duration, display start, background and pixel aspect |
| Layers | Solids, nulls, precomp references, stack order, parent links, start/stretch/in/out mapping and basic switches |
| Transforms | Saved numeric anchor, position, scale, rotation, orientation and opacity |
| Animation | Original tick times, numeric/path/color values, stored spatial handles; verified Linear code; other interpolation modes warn and fall back to linear |
| Text | Static content, Unicode-aware style ranges, fonts, basic character/paragraph styles and text box bounds; font matching/layout remains unverified |
| Shapes | Ordered nested groups, transforms, rectangles, ellipses, Bezier paths, fills, basic strokes and trim paths; saved enable flags |
| Masks | Verified Add mode, enable state, inversion/locking, path/open closure, feather/opacity/expansion; source-backed path coordinates scaled by source dimensions |
| Compositing | Explicit layer blend-code mapping, saved preserve-transparency; Alpha/Luma and inverse mattes with legacy/explicit source resolution and cycle checks |
| Gaussian Blur | Blurriness and its numeric keys, static dimensions/repeat-edge, instance and layer enable switches; narrow definition-derived fallback with source warnings |
| Tracking animator | Static Tracking Type 1 / Amount with one percentage range (Start/End/Offset); elided Advanced uses target defaults with warning; other animator layouts omitted |
| Expressions | Mapped transform expression text preserved, disabled |
| Footage | Missing placeholders, original reference in report; active media paths left empty for manual relinking |
| Other layer kinds | Camera/light/3D-model null placeholders plus warnings |

AE stretch ratios become EffectCraft percentages. In/out source times are mapped
through start and stretch, with reversed bounds ordered. Non-default timing is
reported as requiring AE visual comparison. EffectCraft Tick conversion does not
round keyframes to frames. Saved 16.16 rates remain rational rather than being
silently snapped to a broadcast rate.

Layer scale/opacity and mask opacity convert saved ratios to percentages. Shape
percent values keep their own stored units. Numeric color properties convert
ARGB 0..255 to RGBA 0..1; text-engine colors already use 0..1. Shape paths restore
bounds-space points and convert absolute controls to relative handles. Source-backed
mask paths additionally need the source dimensions; shape-layer masks use pixels.

Other effects, animated Gaussian selectors, effect compositing options, advanced shape operators/gradients, source-text animation, separated-position animation,
nested matte equivalence, color management, markers, work area, render settings and
advanced switches remain outside the mapping. Unsupported properties and
reader diagnostics are reported with source offsets. Missing transform static
values use EffectCraft defaults with warnings when a property record exists;
absent property records retain target defaults. Unverified interpolation and
automatic/roving flags are reported. Invalid required values, duplicate IDs,
duplicate key times, missing/cyclic parents and cyclic precomps return errors.

No media probing, scripts, plugins or expressions run during conversion. The
reader remains a read-only format library; ecproj output lives in this independent
consumer. Its default dependency graph uses EffectCraft's data crates without
building the application or FilmCraft codecs. The optional `render-check` feature
adds the real CPU renderer for integration tests.

Gaussian parameter values prefer explicit instance data. Omitted parameters may
use only the observed Gaussian definition layouts: zero blurriness, dimensions
with stored fields 1/0 and three options, or matching boolean fields. A nonempty
instance table is not filled from another scope; an empty table requires a unique
project catalog entry. Conflicting or unsupported definitions omit the effect
with a warning. Every definition-derived value reports its scope and byte offset;
this is a provisional consumer interpretation, not a general reader default API.
Unknown enable state imports disabled. EffectCraft's Gaussian kernel is approximate;
no AE pixel-equivalence claim is made. Popup/checkbox animation is not mapped.
Nonempty or incompletely decoded effect compositing-option groups omit the
Gaussian instance with a warning; importing it at full strength would be misleading.

Saved shape/operator and mask enable flags are preserved. The target renderer
does not evaluate outer contents/mask-container enable flags, so disabled source
containers disable their imported children with a report warning. Original
individual flags remain recoverable from the AEP. Unknown per-node flags retain
the target default with a warning.

## Checks

```powershell
cargo fmt --manifest-path examples/effectcraft/Cargo.toml --check
cargo test --locked --manifest-path examples/effectcraft/Cargo.toml
cargo clippy --locked --manifest-path examples/effectcraft/Cargo.toml --all-targets -- -D warnings
$env:AEP_FIXTURE_ROOT = "../aep-parser/test_data/fixtures"
$env:AEP_DATA_ROOT = "../aep-parser/data"
cargo test --locked --manifest-path examples/effectcraft/Cargo.toml real_files -- --ignored --nocapture
cargo test --locked --manifest-path examples/effectcraft/Cargo.toml --features render-check -- --include-ignored --nocapture
```

The optional real-file checks read local fixtures in place. They include original
AE2020/2025 authored path geometry and sampled animation, mixed text runs, normalized
open masks, color animation, layer-unit conversion and a native flagged-keyframe
regression. A real EffectCraft CPU render checks a transformed rectangle's color,
opacity and pixel coverage. These are targeted integration checks, not AE-render
comparison, desktop UI acceptance, web support or full AE equivalence.

Gaussian checks also cover the native AE2025 elision fixture, definition conflicts,
disabled stacks and saved blurriness key values/times. An isolated CPU render checks
horizontal-only spreading, repeat-edge behavior and disabling the effect. The render
changes only scene placement and the target repeat-edge control for comparison;
it is not an unmodified full-scene AE comparison.
Visibility regressions alter enable bits only in memory on existing native
fixtures. Model checks cover shapes and masks; CPU checks verify that disabling
the shape root, group, inner contents, rectangle or fill leaves no painted pixels.

Layer blend codes map by name to the target formulas; enum mapping does not prove
AE pixel equivalence, especially classic modes, dissolve, HDR and color-managed
projects. Unknown codes warn and fall back to Normal/no matte. Explicit absent
matte-source IDs never fall back to the layer above; unresolved empty selections
warn, while dangling nonzero IDs and cyclic matte graphs fail import. Source
visibility is preserved independently from use as a matte. Nested mattes warn.
Native AE 2020/2025 reference checks and isolated CPU checks cover four matte
modes, including hidden matte sources. No full AE-image comparison is claimed.

## Next integration steps

1. Verify remaining interpolation codes, easing/automatic flags and non-default timing
   against controlled authored projects and sampled AE values.
2. Expand text/shape/mask coverage and map effects by explicit parameter tables,
   respecting per-instance saved values versus shared effect-definition defaults.
3. Add trusted media relinking and detailed per-feature fidelity checks.
4. Once upstream accepts the scope, reuse `import(bytes)` from an undoable
   EffectCraft command and expose its report through UI/CLI/MCP.

EffectCraft's `docs/gaps.md` G4 reserves native AEP integration for a maintainer
decision on clean-room scope. This external consumer does not change upstream
policy or claim approval. Reader format provenance remains in
[PROVENANCE.md](../../PROVENANCE.md).

The desktop MCP bridge was used to inspect and render the authored tracking
reference. It contains static tracking settings, not time-varying keyframes.
The target property reads back as 50 and affects glyph spacing; this is a narrow
mapping check, not evidence of general text-animation or AE pixel equivalence.

## Corpus-driven desktop policy

This adapter targets EffectCraft directly; its experimental API may change with
consumer needs. Desktop parsing uses finite limits of one million chunks and
512 MiB decoded allocations (input remains capped at 256 MiB); the reusable
reader's lower defaults are unchanged. Larger projects return a resource error.
Invalid temporal-ease handles are omitted with warnings while key time/value
remain linear. Undecoded key values are omitted locally and reported, preserving
other decoded keys and the static/default property value. Neither policy claims
that the missing animation was recovered.

Composition duration uses its saved rational denominator, including fractional
frame rates. Signed display-start values preserve negative frame offsets.
The local batch validation distinguishes conversion, MCP render success, changing
frames, static output and flat frames; successful conversion is not fidelity proof.
