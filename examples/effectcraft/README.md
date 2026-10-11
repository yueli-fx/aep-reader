# AEP → EffectCraft experimental consumer

A standalone adapter using aep-reader and EffectCraft's project model. It produces
`.ecproj` and a per-property `.ecproj.import.json` report. Serialization is checked
by reopening with `Project::from_json()`, the same model loader used by the editor.
This is an experimental importer with explicit limits, not full AE rendering parity.

The authoritative [capability matrix](../../CAPABILITIES.md) separates reader
support, adapter support, conflicts and unverified behavior.

## Windows desktop use

Double-click `AEP转EffectCraft.exe` to choose an AEP, or drag an AEP onto the
converter executable. The converter creates a project and report beside the
input, then shows their location. Existing files are preserved with an
`-import-N` suffix. Canceling the picker creates nothing; failures show a dialog.
Open the resulting `.ecproj` in EffectCraft.

Dropping an AEP directly into the EffectCraft editor is not implemented. No media,
plugin, expression, script, profile or OCIO reference is executed or loaded by the
converter. Missing-media placeholders retain references in the report for future
explicit relinking. The source AEP is never modified.

## Build and run

The adapter requires Rust 1.95+; the reusable reader requires Rust 1.85.
Place the EffectCraft checkout beside this repository, named `effectcraft`.
The API baseline is `813c7c4650f0d4c805da620c8d5837c9600ad2e3` (0.7.0).
Cargo.lock pins registry dependencies, not sibling path dependencies.

From the aep-reader root in PowerShell:

```powershell
$env:CARGO_TARGET_DIR = "$PWD/.task/effectcraft-local/target"
cargo run --locked --manifest-path examples/effectcraft/Cargo.toml -- input.aep output.ecproj
cargo build --locked --release --manifest-path examples/effectcraft/Cargo.toml
```

The release executable is under the chosen target directory. Both explicit output
paths must be new. The default build uses project/keyframe/time/color crates and
has no application-host or rendering dependency. `render-check` adds the CPU
renderer for integration tests.

## Current mappings

- Project hierarchy, timing, work area, shutter metadata, bit depth and sample rate.
- Solids, nulls, precomps, transforms, separated position axes, key-driven time remap,
  named blend modes, four track mattes and selected switches.
- Static styled text; common numeric/color text animator channels; ordered percent
  or index range selectors and known Advanced parameters.
- Ordered shapes, paths, fills/strokes, trim, static stars/polygons and repeaters,
  gradient snapshots, bounded round/pucker/twist/zigzag/merge/single-copy offset.
- Seven named mask modes; real camera/light layers, saved options and explicit
  material shadow/light switches.
- Limited Gaussian Blur mapping, including parameter keys and enable state.

SourceText content changes, wiggly/expression selectors, per-character 3D,
other effects, media relinking and color-management pipelines remain unsupported.
Consult the matrix for the narrower limits of each implemented area.

## Values, animation and defaults

Saved ticks are converted without frame rounding. Layer scale/opacity and mask
opacity use stored ratios and become target percentages; text/shape animator
percentages retain their own units. Numeric colors change from ARGB 0..255 to RGBA
0..1. Paths restore bounds-space anchors and convert absolute controls to relative
handles; source-backed masks additionally use source dimensions.

Unknown interpolation codes use a reported linear fallback. Invalid ease handles
are omitted while valid key times/values remain; undecoded values are reported.
These fallbacks do not recover the missing animation. Start/stretch mapping and
active-camera/render behavior still need AE comparison. Expressions retained on
supported properties are disabled; expression-driven specialized operators or
animators are omitted where their saved result is not a safe substitute.

Elided defaults are explicit consumer conventions, reported separately from saved
values. Gradient midpoints use piecewise-linear inserted stops. Marker durations
with unverified layouts become point markers. Randomized text order, text selector
shapes, gradient easing and path contours use target algorithms; AE equivalence
is unverified.

Text animators can combine supported channels and up to 32 ordered ranges. Selector
units/based-on/mode/shape and randomize are static; amount/smoothness/ease/seed and
range bounds accept numeric keys. Index ranges require a saved end. A malformed,
unknown or executable selector omits the entire animator with a local diagnostic;
its partial property counts and consumed offsets are not committed.

Gaussian instance values take precedence over definitions. Only observed Gaussian
definition layouts supply reported fallback values; ambiguous/mismatched definitions,
unknown selectors and nonempty/incomplete compositing-option groups omit the effect.
This is not a general effect-default decoder.

## Budgets and recovery

The CLI reads at most 256 MiB. The desktop adapter allows a 768 MiB cumulative
parser-allocation budget and one million chunks; the reader's default allocation
budget remains 128 MiB. Accounting excludes the caller's input and allocator overhead.
Large valid projects may still exceed these limits.

Shape expansion is conservatively estimated across nested groups. Repeaters have
separate copy/scale limits. Excessive modifier stacks omit operators with warnings;
complex Boolean and offset processing still depends on the target implementation.
Offset supports one copy with the source default step because multi-copy offset
semantics differ between source and target. Invalid shape geometry is omitted
locally instead of destroying the complete import.

The byte API `import()` rejects missing parents. `import_with_options()` can opt
into `ImportOptions { recover_missing_parents: true }`; the desktop converter uses
this recovery. Recovered layers keep their local transform and report that the
missing parent's world-space placement cannot be reconstructed. Cycles remain errors.

## Checks

```powershell
$env:CARGO_TARGET_DIR = "$PWD/.task/effectcraft-check/target"
cargo fmt --manifest-path examples/effectcraft/Cargo.toml --check
cargo test --locked --manifest-path examples/effectcraft/Cargo.toml --all-features
cargo clippy --locked --manifest-path examples/effectcraft/Cargo.toml --all-targets --all-features -- -D warnings
```

Ignored native checks require local original files. Set `AEP_FIXTURE_ROOT` to the
fixture directory, `AEP_DATA_ROOT` to the corpus root, and `AEP_SHAPE_FIXTURE_ROOT`
to the shape-reference directory, then add `-- --include-ignored` to the test
command. No original assets are distributed or generated by these tests. Remove
regenerable task caches after inspection; retain deliverable projects and reports.

[Validation](../../VALIDATION.md) documents the checked corpus and desktop evidence.
A successful conversion or a changing frame does not prove a faithful AE image.
