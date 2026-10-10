#![allow(clippy::expect_used)]
use std::process::{Command, Stdio};

#[test]
fn help_exits_successfully_and_explains_desktop_usage() {
    let out = Command::new(env!("CARGO_BIN_EXE_aep-effectcraft"))
        .arg("--help")
        .output()
        .expect("run CLI");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Double-click") && text.contains(".ecproj"));
}

#[test]
fn explicit_paths_report_errors_without_opening_desktop_dialogs() {
    let out = Command::new(env!("CARGO_BIN_EXE_aep-effectcraft"))
        .args(["does-not-exist.aep", "output.wrong-extension"])
        .stdin(Stdio::null())
        .output()
        .expect("run CLI");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("output must end in .ecproj"));
}
