# aep-reader

A read-only After Effects `.aep` parser in pure safe Rust. Read project structure,
compositions, layers, properties, animation and footage metadata directly from
bytes, without installing or launching After Effects.

The default crate has no dependencies. Optional `serde` support serializes the
project model. The byte API supports native and WebAssembly builds.
The minimum supported Rust version is 1.85.0.

## Use

The crate is currently unpublished. Add it to a local consumer with a path dependency:

```toml
[dependencies]
aep-reader = { path = "../aep-reader" }
```

A Cargo `.crate` source archive can also be extracted and used with the same path
dependency. It contains the library, public-boundary tests, Rust examples, licenses
and reading/validation documentation. A full source checkout additionally includes
the standalone consumer, development checks and CI workflow.

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read("project.aep")?;
    let document = aep_reader::parse(&bytes)?;
    for comp in &document.project.compositions {
        println!("{}: {:?} x {:?}", comp.name, comp.width, comp.height);
        for layer in &comp.layers {
            println!("  {}: {:?}", layer.name, layer.kind);
        }
    }
    Ok(())
}
```

Inspect a local project as JSON:

```sh
cargo run --example inspect --features serde -- project.aep
```

For a standalone application using the public API, see the
[independent consumer](examples/consumer/README.md). It demonstrates bounded file
input, property/keyframe queries, incomplete-decoding diagnostics and optional JSON:

```sh
cargo run --manifest-path examples/consumer/Cargo.toml -- project.aep --property "ADBE Position"
```

## Reading capabilities

An experimental [EffectCraft consumer](examples/effectcraft/README.md) converts
project structure, transforms, static text and basic shapes/masks into `.ecproj`, with an explicit
import report. It requires a sibling EffectCraft checkout and has limited mapping
coverage; parsing success does not imply faithful rendering.

| Area | Available data |
|---|---|
| Project | Items, folder hierarchy, panel order, settings, color-management references and bit depth |
| Compositions and layers | Dimensions, timing, renderer identity, switches, source/parent/matte references and alternate sources |
| Properties and animation | Ordered property tree, names, numeric values, keyframes, easing, tangents, expressions, units, ranges and flags |
| Effects | Effect identities, saved values, separate project/instance parameter definitions and raw default fields |
| Masks and shapes | Bezier paths, bounds, correctly paired control points and animation; mask metadata and borrowed shape/operator views |
| Text | Static text, fonts/axes, styles, checked UTF-16 run ranges, paragraphs, box bounds and kerning |
| Gradients and panels | Color/alpha gradient snapshots, validated animation times/interpolation codes and Essential Graphics controller metadata |
| Render queue and footage | Saved queue/output settings, format options, footage paths, sequence descriptors, solids, placeholders and audio metadata |

See [READING-COVERAGE.md](READING-COVERAGE.md) for API details and the limits of
each area. Numeric values retain their stored units. Default-value queries
distinguish file-contained values from tagged reader conventions; they never
replace saved values. Optional fields and uncertain derived states remain absent
or unknown when there is insufficient evidence.

## Incomplete decoding

Unsupported data remains accessible through the borrowed chunk tree and source
spans. Recoverable decoding issues produce `project.diagnostics`; individual
property nodes also expose `DecodeStatus`. Check descendants as well as the
group itself when deciding what an importer can use.

Gradient easing/evaluation, source-text animation,
some effect/codec payloads and newer file layouts remain incomplete. Marker
duration uses a compatibility layout and is explicitly marked unverified.
Successful parsing does not establish AE-equivalent rendering.

The package reads binary RIFX/Egg! `.aep` files. Writing, rendering, media decoding,
expression/plugin execution, `.aepx` and `.ffx` are outside its current scope.
Media, font, profile and OCIO paths remain references; the parser does not open them.

`parse_text_document()` reads a standalone `btds` payload and
`parse_gradient_xml()` reads the supported data-only gradient XML subset. Both
have `_with_limits` variants. `TextRange::slice()` respects UTF-16 boundaries,
and `BezierPath::bounds_vertices()` restores absolute control points in the
stored bounds' coordinate space. Source-backed masks may still require source
size conversion; `layer_vertices()` remains a compatibility alias. `FrameRate`
provides checked seconds/frame conversions.

## Resource limits and original bytes

`parse_with_limits()` bounds input size, chunk size/count, nesting depth,
cumulative keyframes and conservatively accounted parser allocations. The
allocation budget excludes the caller's input buffer and allocator overhead.
Malformed containers and resource-limit violations return `Error`.

The input buffer must outlive `Document`. `document.bytes(node.source)` returns
the original bytes for a source span, and `document.root` exposes the chunk tree.
`document.trailing` preserves bytes after the container; `xmp_packet()` exposes
them as UTF-8 when possible without interpreting XML.

## Development

```sh
cargo fmt --check
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo check --target wasm32-unknown-unknown --all-features
python -B tools/check_consumer.py --toolchain stable
```

[CI](.github/workflows/ci.yml) configures stable checks on Windows, Linux and macOS,
minimum-Rust checks, Wasm compilation, standalone consumer checks and source-package
verification. `cargo package --locked --all-features --allow-dirty` builds and
verifies a source archive; `tools/check_source_package.py` checks that its contents
exclude fixtures, caches, templates and toolchains. The consumer
check creates an ignored `.task` directory for its generated inputs, logs and build
outputs; remove it after reviewing the results.

[VALIDATION.md](VALIDATION.md) records test coverage, checked toolchains and
remaining verification limits. [COMPARISON.md](COMPARISON.md) describes research and comparison boundaries.

## License

MIT OR Apache-2.0, at your option. See [LICENSE-MIT](LICENSE-MIT),
[LICENSE-APACHE](LICENSE-APACHE) and [PROVENANCE.md](PROVENANCE.md).

This project is not affiliated with Adobe. After Effects is an Adobe trademark.
