# Provenance

The Rust reader is maintained by the Yueli FX project. Its initial format
knowledge and decoding rules were adapted from the maintainer's private Go
reader, including container, serializer and scene-reading behavior. That work
contains both project observations and notes identifying prior reference
implementations; a Rust translation does not erase that lineage.
The property-unit catalog follows a prior reference parser's `data/units.py`.
Transform defaults remain explicit conventions or approximations rather than
file values. This package does not claim independent clean-room provenance.

The Rust package is licensed under MIT OR Apache-2.0. This license applies to
this package; reference repositories, third-party components and original test
fixtures retain their own licensing terms.

No Go writer templates, serialized effect/shape seeds, private corpus projects,
commercial sample projects, Adobe application bundle contents or plugin code were
copied into this repository. Tests construct synthetic chunks in code. Local
comparison reads existing fixtures in place and does not redistribute them.

The default crate has no third-party dependencies. Optional serialization uses
Serde; development JSON tooling uses serde_json and their Cargo-resolved
dependencies. Those components retain their own licenses and notices. Consult
Cargo metadata and upstream license files before shipping binaries or vendoring.

Format provenance is independent of runtime dependencies. The byte API has no
Go, application-host or FFI dependency. Consumers can inspect the format lineage
here and the verification evidence in [COMPARISON.md](COMPARISON.md).
