//! A standalone application using only aep-reader's public API.
use aep_reader::{DecodeStatus, Document, Limits, PropertyNode, Value};
#[cfg(feature = "json")]
use std::io;
use std::{env, error::Error, ffi::OsString, fs::File, io::Read, path::PathBuf, process::ExitCode};

const USAGE: &str =
    "usage: aep-reader-consumer PROJECT.aep [--property MATCH_NAME] [--max-bytes N] [--json]";

struct Args {
    path: PathBuf,
    property: String,
    max_bytes: usize,
    json: bool,
}
fn args() -> Result<Option<Args>, Box<dyn Error>> {
    let mut input = env::args_os().skip(1);
    let Some(path) = input.next() else {
        return Err(USAGE.into());
    };
    if path == "--help" || path == "-h" {
        println!("{USAGE}");
        return Ok(None);
    }
    let mut args = Args {
        path: path.into(),
        property: "ADBE Opacity".into(),
        max_bytes: Limits::default().max_input_bytes,
        json: false,
    };
    while let Some(flag) = input.next() {
        if flag == "--property" {
            args.property = utf8(input.next(), "--property requires a match name")?;
        } else if flag == "--max-bytes" {
            args.max_bytes =
                utf8(input.next(), "--max-bytes requires a positive integer")?.parse()?;
            if args.max_bytes == 0 {
                return Err("--max-bytes must be positive".into());
            }
        } else if flag == "--json" {
            args.json = true;
        } else {
            return Err(format!("unknown option {:?}; {USAGE}", flag).into());
        }
    }
    Ok(Some(args))
}
fn utf8(value: Option<OsString>, missing: &'static str) -> Result<String, Box<dyn Error>> {
    value
        .ok_or(missing)?
        .into_string()
        .map_err(|_| "option value is not UTF-8".into())
}
fn value_text(value: Option<&Value>) -> String {
    match value {
        None => "<unavailable>".into(),
        Some(Value::Numbers(values)) => {
            let values: Vec<_> = values
                .iter()
                .map(|n| match n.value {
                    Some(v) => v.to_string(),
                    None => format!("{:?}", n.non_finite),
                })
                .collect();
            format!("[{}]", values.join(", "))
        }
        Some(Value::Path(path)) => format!(
            "path: {} vertices, closed={:?}",
            path.vertices.len(),
            path.closed
        ),
    }
}
fn incomplete(nodes: &[PropertyNode]) -> usize {
    nodes
        .iter()
        .map(|n| usize::from(n.decode_status != DecodeStatus::Decoded) + incomplete(&n.children))
        .sum()
}
fn diagnostics(doc: &Document<'_>) {
    let count: usize = doc
        .project
        .layers()
        .map(|l| incomplete(&l.properties))
        .sum();
    eprintln!(
        "incomplete nodes: {count}; diagnostics: {}",
        doc.project.diagnostics.len()
    );
    for d in &doc.project.diagnostics {
        eprintln!("{} at byte {}: {}", d.code, d.offset, d.message);
    }
}
fn inspect(doc: &Document<'_>, match_name: &str) {
    let p = &doc.project;
    println!(
        "project: {} compositions, {} layers, {} footage items",
        p.compositions.len(),
        p.layers().count(),
        p.footage.len()
    );
    for c in &p.compositions {
        println!(
            "composition {}: {} ({:?} x {:?})",
            c.id, c.name, c.width, c.height
        );
        for l in &c.layers {
            println!("  layer {}: {} ({:?})", l.id, l.name, l.kind);
            let Some(n) = l.node(match_name) else {
                println!("    {match_name}: <absent>");
                continue;
            };
            println!(
                "    {}: status={:?}, units={:?}, modified={:?}",
                n.match_name,
                n.decode_status,
                n.units().map(|u| u.as_str()),
                n.is_modified()
            );
            let Some(prop) = &n.property else {
                println!("    no decoded property stream");
                continue;
            };
            println!("    static: {}", value_text(prop.static_value.as_ref()));
            for k in &prop.keyframes {
                println!(
                    "    key: ticks={}, seconds={}, value={}, interpolation={}/{}",
                    k.time_ticks,
                    k.time_seconds,
                    value_text(k.value.as_ref()),
                    k.in_interpolation,
                    k.out_interpolation
                );
            }
        }
    }
    for f in &p.footage {
        println!(
            "footage {}: {} ({}) path={:?}",
            f.id,
            f.name,
            f.main_source().kind(),
            f.path
        );
    }
}
#[cfg(feature = "json")]
fn json(doc: &Document<'_>) -> Result<(), Box<dyn Error>> {
    serde_json::to_writer_pretty(io::stdout().lock(), &doc.project)?;
    Ok(())
}
#[cfg(not(feature = "json"))]
fn json(_: &Document<'_>) -> Result<(), Box<dyn Error>> {
    Err("JSON output requires building the consumer with --features json".into())
}
fn run() -> Result<(), Box<dyn Error>> {
    let Some(args) = args()? else {
        return Ok(());
    };
    let limits = Limits {
        max_input_bytes: args.max_bytes,
        ..Limits::default()
    };
    // Read at most the limit plus one byte, allowing the parser to report excess
    // input without loading the rest of a large file. No referenced media is opened.
    let mut bytes = Vec::new();
    File::open(args.path)?
        .take(u64::try_from(limits.max_input_bytes)?.saturating_add(1))
        .read_to_end(&mut bytes)?;
    let doc = aep_reader::parse_with_limits(&bytes, &limits)?;
    diagnostics(&doc);
    if args.json {
        json(&doc)
    } else {
        inspect(&doc, &args.property);
        Ok(())
    }
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
