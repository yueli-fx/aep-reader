#![cfg_attr(windows, windows_subsystem = "windows")]
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

fn convert(input: &Path, output: &Path) -> Result<String, Box<dyn std::error::Error>> {
    if output.extension().and_then(|s| s.to_str()) != Some("ecproj") {
        return Err("output must end in .ecproj".into());
    }
    let mut sidecar = output.as_os_str().to_os_string();
    sidecar.push(".import.json");
    let sidecar = PathBuf::from(sidecar);
    if output.exists() || sidecar.exists() {
        return Err("output or report already exists; choose a new output path".into());
    }
    let limit = 256 * 1024 * 1024_u64;
    let mut bytes = Vec::new();
    File::open(input)?.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err("AEP exceeds 256 MiB input limit".into());
    }
    let imported = aep_effectcraft::import(&bytes)?;
    let json = imported.project.to_file_json()?;
    let report = serde_json::json!({
        "source": input.to_string_lossy(),
        "source_bytes": bytes.len(),
        "adapter_version": env!("CARGO_PKG_VERSION"),
        "report": imported.report,
    });
    let report = serde_json::to_vec_pretty(&report)?;
    // Exclusive creation also closes the existence-check race. Both files are task-owned
    // only after creation, so a partial write can be rolled back without touching user data.
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    let result = (|| -> std::io::Result<()> {
        let mut report_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&sidecar)?;
        let written = (|| -> std::io::Result<()> {
            report_file.write_all(&report)?;
            report_file.sync_all()?;
            out.write_all(json.as_bytes())?;
            out.sync_all()
        })();
        drop(report_file);
        if written.is_err() {
            std::fs::remove_file(&sidecar)?;
        }
        written
    })();
    drop(out);
    if let Err(error) = result {
        std::fs::remove_file(output)?;
        return Err(error.into());
    }
    println!(
        "Created {}: {} comps, {} layers, {} keys, {} warnings. Report: {}",
        output.display(),
        imported.report.compositions,
        imported.report.layers,
        imported.report.keyframes,
        imported.report.warnings.len(),
        sidecar.display()
    );
    Ok(format!(
        "已生成 EffectCraft 工程：\n{}\n\n请在 EffectCraft 中打开该 .ecproj 文件。\n转换提示：{} 条，详见：\n{}",
        output.display(),
        imported.report.warnings.len(),
        sidecar.display()
    ))
}

#[cfg(windows)]
fn unused_output(input: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let first = input.with_extension("ecproj");
    let stem = input.file_stem().ok_or("AEP 文件名无效")?;
    for n in 0..10_000 {
        let output = if n == 0 {
            first.clone()
        } else {
            let mut name = stem.to_os_string();
            name.push(format!("-import-{n}.ecproj"));
            input.with_file_name(name)
        };
        let mut report = output.as_os_str().to_os_string();
        report.push(".import.json");
        if !output.exists() && !Path::new(&report).exists() {
            return Ok(output);
        }
    }
    Err("同名输出过多，请移动 AEP 后再试".into())
}

#[cfg(windows)]
fn desktop(input: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let input = match input {
        Some(p) => p,
        None => match rfd::FileDialog::new()
            .set_title("选择要转换为 EffectCraft 工程的 AEP")
            .add_filter("After Effects 工程", &["aep"])
            .pick_file()
        {
            Some(p) => p,
            None => return Ok(()),
        },
    };
    let result = unused_output(&input).and_then(|output| convert(&input, &output));
    let (title, message, level) = match &result {
        Ok(message) => ("转换完成", message.clone(), rfd::MessageLevel::Info),
        Err(error) => (
            "转换失败",
            format!("{}\n\n{}", input.display(), error),
            rfd::MessageLevel::Error,
        ),
    };
    rfd::MessageDialog::new()
        .set_title(title)
        .set_description(message)
        .set_level(level)
        .show();
    result.map(|_| ())
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [flag] if flag == "--help" || flag == "-h" => {
            println!(
                "AEP to EffectCraft converter\nDouble-click to choose an AEP, or drag an AEP onto this executable.\nOutput: .ecproj beside the AEP; existing files are preserved.\nAutomation: aep-effectcraft input.aep output.ecproj"
            );
            Ok(())
        }
        #[cfg(windows)]
        [] => desktop(None),
        #[cfg(windows)]
        [input] => desktop(Some(PathBuf::from(input))),
        [input, output] => convert(Path::new(input), Path::new(output)).map(|_| ()),
        _ => Err("usage: aep-effectcraft input.aep output.ecproj".into()),
    }
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn automatic_output_preserves_existing_project_and_report()
    -> Result<(), Box<dyn std::error::Error>> {
        let parent = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.task");
        std::fs::create_dir_all(&parent)?;
        let root = parent.join(format!("desktop-name-test-{}", std::process::id()));
        std::fs::create_dir(&root)?;
        let input = root.join("中文 input.aep");
        let first = super::unused_output(&input)?;
        assert_eq!(first, root.join("中文 input.ecproj"));
        std::fs::write(&first, b"keep project")?;
        let report = root.join("中文 input-import-1.ecproj.import.json");
        std::fs::write(&report, b"keep report")?;
        assert_eq!(
            super::unused_output(&input)?,
            root.join("中文 input-import-2.ecproj")
        );
        assert_eq!(std::fs::read(&first)?, b"keep project");
        assert_eq!(std::fs::read(&report)?, b"keep report");
        std::fs::remove_file(first)?;
        std::fs::remove_file(report)?;
        std::fs::remove_dir(root)?;
        Ok(())
    }
}
