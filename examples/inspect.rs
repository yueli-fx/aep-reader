use std::{
    env,
    fs::File,
    io::{self, Read},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args_os()
        .nth(1)
        .ok_or("usage: cargo run --example inspect --features serde -- project.aep")?;
    let limits = aep_reader::Limits::default();
    let mut bytes = Vec::new();
    File::open(path)?
        .take(u64::try_from(limits.max_input_bytes)?.saturating_add(1))
        .read_to_end(&mut bytes)?;
    let doc = aep_reader::parse_with_limits(&bytes, &limits)?;
    serde_json::to_writer_pretty(io::stdout().lock(), &doc.project)?;
    Ok(())
}
