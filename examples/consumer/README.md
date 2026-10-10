# Independent application example

This is a standalone Cargo workspace using `aep-reader` as a path dependency.
It calls the public API to read compositions, layers, footage, a selected property
and its keyframes. It reports incomplete decoding and errors without accessing
referenced media. The default application has no third-party dependencies;
the optional `json` feature enables Serde and serde_json.

## Run

From the aep-reader repository root:

```sh
cargo run --manifest-path examples/consumer/Cargo.toml -- project.aep
cargo run --manifest-path examples/consumer/Cargo.toml -- project.aep --property "ADBE Position"
cargo run --manifest-path examples/consumer/Cargo.toml -- project.aep --property "ADBE Opacity" --max-bytes 67108864
cargo run --manifest-path examples/consumer/Cargo.toml -- --help
```

Output includes project/layer summaries, the selected property's units and decode
status, its saved static value and keyframes with original ticks and seconds.
`--property` defaults to `ADBE Opacity` and selects the first matching node in
each layer. Missing properties print `<absent>`; undecoded values print
`<unavailable>`. Stored values and enums keep their original conventions.

Warnings and incomplete-node counts go to stderr. Successful parsing with
partially decoded data returns success; callers must review diagnostics before
using fields in an importer. File errors, malformed input, exceeded limits and
invalid options return a nonzero exit status with an error message.

## Bounded input

`--max-bytes` changes `Limits::max_input_bytes`. Other limits keep their defaults.
The file read stops after at most that limit plus one byte, allowing the parser
to report an input-size violation without reading the remainder of a large file.
The input buffer stays alive for the borrowed `Document` and source spans.

For additional limits, adapt the `Limits` value in [src/main.rs](src/main.rs).
Optional fields, unknown derived states and file defaults remain distinct in the
public model. The application performs no expression or animation evaluation.

## JSON

```sh
cargo run --manifest-path examples/consumer/Cargo.toml --features json -- project.aep --json > project.json
```

JSON contains the complete exposed project model. Diagnostics still go to stderr,
so stdout remains valid JSON. `--json` without the feature produces an explicit
error. This feature does not change parsing or the treatment of partial data.

## Check the consumer

From the repository root:

```sh
python -B tools/check_consumer.py --toolchain stable
```

The check builds the example independently in both feature configurations and
exercises synthetic inputs, Unicode names, static/animated queries, diagnostics,
JSON output, malformed input and resource limits. It also checks the default
runtime dependency tree. Add `--fixture path/to/project.aep` for local real-file
checks; assets are read in place. `--offline` uses cached Cargo dependencies.
Generated build/test files are kept in the printed ignored `.task` directory and
can be cleaned after review. See [validation](../../VALIDATION.md) for the
actual verified toolchains and current support boundary.
