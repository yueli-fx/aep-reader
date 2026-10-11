# Validation

## Reader and delivery checks

The public tests construct bounded synthetic inputs and exercise the public API,
including malformed/truncated containers, resource budgets, metadata, text,
paths, effects, time ratios and incomplete decoding. Optional ignored checks
read locally supplied originals without copying them into the repository.

Local Windows checks on 2026-10-11 passed:

- Reader: 71 integration tests and one doctest, plus the separately enabled
  original-file effect-catalog test; formatting and strict Clippy.
- Rust 1.85: all-feature tests and a `wasm32-unknown-unknown` all-feature build.
- Standalone consumer: 13 checks, including default dependency boundaries,
  structured output and malformed input.
- Source package: builds successfully; 40 allowlisted source/document/license
  files, excluding native fixtures, Go sources, toolchains and task caches.
- EffectCraft adapter: 73 tests with all features and local native checks enabled,
  including CPU rendering; formatting and strict Clippy; release executable build.

The adapter has its own Rust 1.95+ and sibling-checkout requirements. These do not
change the reusable reader's Rust 1.85 requirement. The Wasm build checks reader
compilation, not a browser UI or the native desktop adapter.

## Corpus and actual editor

The final converter was rerun against 2,734 local `.aep`-named files. All 2,197
binary AEP projects converted successfully and reopened through the target model
loader. The other inputs were 536 Git LFS pointers and one invalid header; these
are reported separately from real-project results. Previous failures caused by
missing parents, allocation accounting and path geometry are resolved through
explicit recovery, appropriate adapter budgets and reported local omissions.
Recovered placement and omitted content are not restored source fidelity.

Desktop evidence uses EffectCraft 0.7.0's MCP bridge:

| Check | Scope | Result / limit |
|---|---|---|
| Earlier broad sample | 84 projects, five times each | No open/render errors; 60 changed, nine static nonflat, fifteen flat throughout; predates the latest mappings |
| Shape/operator batch | Nine projects, 32 saved frames | No tool errors; selected operators and active-time content checked; complex bounded omissions remain |
| Final text batch | Six original projects, 18 frames at 0/1/2 seconds | No tool errors; progressive reveal and changing position/scale/color visible; unsupported special selectors explicitly omitted |

Synthetic pixel tests isolate selected mappings such as opacity, mattes and
enable state. They do not compare against AE reference renders. Changing pixels,
successful conversion and successful JSON loading are different evidence from
AE image fidelity. Complex shapes, text layout, random sequences and 3D remain
particularly limited. See the [capability matrix](CAPABILITIES.md).

Private source projects, derived previews, machine paths, reference API inventories
and raw local reports are not distributed. The public corpus counts are a local
observation, not a reproducible public benchmark without separately supplied assets.

## Reproduction

Use an ignored task directory for build outputs. Run `cargo fmt --check`,
`cargo test --all-features`, and
`cargo clippy --all-targets --all-features -- -D warnings`. CI also exercises the
independent consumer, source-package boundaries, minimum Rust and Wasm builds.
See [development](README.md#development) and the
[adapter checks](examples/effectcraft/README.md#checks) for commands and optional
native-fixture environment variables. Local Windows checks do not replace
Linux/macOS CI. Expressions are never executed by this reader or adapter.
