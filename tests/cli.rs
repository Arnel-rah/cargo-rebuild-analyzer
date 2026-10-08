use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::Value;

fn run_cli(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cargo-rebuild-analyzer"))
        .args(arguments)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to execute cargo-rebuild-analyzer")
}

fn run_cli_with_history(arguments: &[&str], history: &PathBuf) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cargo-rebuild-analyzer"))
        .args(arguments)
        .env("CARGO_REBUILD_ANALYZER_HISTORY", history)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to execute cargo-rebuild-analyzer")
}

#[test]
fn prints_help_with_available_commands() {
    let output = run_cli(&["--help"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("history"));
    assert!(stdout.contains("why"));
    assert!(stdout.contains("report"));
    assert!(stdout.contains("--ci"));
    assert!(stdout.contains("--max-rebuilds"));
}

#[test]
fn rejects_unknown_package_before_building() {
    let output = run_cli(&["--package", "missing-package"]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(stderr.contains("unknown package 'missing-package'"));
    assert!(stderr.contains("Available packages"));
}

#[test]
fn rejects_unknown_features_before_building() {
    let output = run_cli(&["--features", "missing-feature"]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(stderr.contains("unknown feature(s): missing-feature"));
    assert!(stderr.contains("Available features"));
}

#[test]
fn report_json_has_machine_readable_summary() {
    let directory = std::env::temp_dir().join(format!(
        "cargo-rebuild-analyzer-test-{}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).expect("failed to create test history directory");
    let history = directory.join("history.json");
    fs::write(
        &history,
        r#"{"builds":[{"timestamp":1,"duration_ms":42,"estimated_wasted_ms":0,"release":false,"features":[],"package":null,"root_package":null,"dependencies":[],"compiled":[]}]}"#,
    )
    .expect("failed to write test history");

    let output = run_cli_with_history(&["report", "--json"], &history);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: Value = serde_json::from_str(&stdout).expect("report should be valid JSON");

    assert!(output.status.success());
    assert!(report.get("crates_analyzed").is_some());
    assert!(report.get("crates_rebuilt").is_some());
    assert!(report.get("duration_ms").is_some());
    assert!(report.get("rebuilt").is_some());
    fs::remove_dir_all(directory).expect("failed to remove test history directory");
}

#[test]
fn unknown_crate_returns_an_error() {
    let output = run_cli(&["why", "missing-crate"]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(stderr.contains("no rebuild recorded for crate 'missing-crate'"));
}

#[test]
fn ci_thresholds_require_ci_mode() {
    let output = run_cli(&["--max-rebuilds", "1"]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert!(stderr.contains("--max-rebuilds"));
    assert!(stderr.contains("--ci"));
}

#[test]
fn ci_mode_accepts_a_generous_rebuild_budget() {
    let directory = std::env::temp_dir().join(format!(
        "cargo-rebuild-analyzer-ci-test-{}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).expect("failed to create test history directory");
    let history = directory.join("history.json");

    let output = run_cli_with_history(&["--ci", "--max-rebuilds", "999"], &history);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(output.status.success(), "stderr: {stderr}");
    assert!(history.exists());
    fs::remove_dir_all(directory).expect("failed to remove test history directory");
}
