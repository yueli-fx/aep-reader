# Validation

The public tests construct bounded synthetic inputs and check the reader's
public API. Optional ignored checks read locally supplied originals without
copying them into the repository. Tests cover malformed containers, resource
budgets, metadata, text, paths, effects, time ratios and incomplete decoding.

Local integration evidence (2026-10-10): 2,184 of 2,197 binary AEPs converted
through the experimental EffectCraft adapter. An additional 536 files were Git
LFS pointers and one had an invalid header. Thirteen real inputs remained
failures: eleven parent references, one allocation budget and one path geometry.
84 projects were sampled through EffectCraft 0.7.0's desktop MCP bridge at five
times each. No open/render call failed; 60 projects changed, nine were static
and nonflat, and fifteen had flat samples throughout. These measurements do
not establish AE image fidelity or complete feature support. Private source
projects, derived previews, machine paths and raw reports are not distributed.

Run `cargo fmt --check`, `cargo test --all-features`, and
`cargo clippy --all-targets --all-features -- -D warnings`. CI also exercises the
independent consumer, source-package boundaries, minimum Rust and Wasm builds.
See [adapter scope](examples/effectcraft/README.md) for separate integration
checks and known limitations. Windows local checks do not replace Linux/macOS CI.

Missing media, unsupported effects, advanced animation and expressions remain
important limitations. Expressions are never executed by this reader or adapter.
