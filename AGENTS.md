# Repository instructions

Build a reusable, read-only AEP library in pure safe Rust. Keep consumer application
adapters outside the format reader. Describe the public API in terms of this package;
keep research lineage and reference comparisons in their dedicated documents.
Preserve access to unknown bytes and report
incomplete decoding. Never execute expressions, plugins, scripts, or media paths.
Do not add file writers, AE generation templates, or proprietary sample assets.

Use the installed Flightdeck skill for durable work; recover from
`flightdeck/deck.md` and `flightdeck/knowledge/index.md`. Keep `/flightdeck/`
ignored; these records are local only. Each document folder needs an index.

Check with `cargo fmt --check`, `cargo test --all-features`, and
`cargo clippy --all-targets --all-features -- -D warnings`. Changes to parsing
need public-boundary malformed-input tests and representative real-file checks.
Building for `wasm32-unknown-unknown` is required before declaring web support.

On Windows, run background helpers without windows. Task artifacts and build
outputs belong in a unique `.task/` directory; remove regenerable task artifacts
after stopping task-owned processes and checking paths and Git state.

Creating a repository does not authorize commit or push. Obtain each separately,
for the named repository and branch. Keep changes local until authorized.
