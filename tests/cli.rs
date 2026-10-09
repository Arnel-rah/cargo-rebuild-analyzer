use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

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

fn run_cli_in_project(arguments: &[&str], project: &PathBuf, history: &PathBuf) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cargo-rebuild-analyzer"))
        .args(arguments)
        .env("CARGO_REBUILD_ANALYZER_HISTORY", history)
        .current_dir(project)
        .output()
        .expect("failed to execute cargo-rebuild-analyzer")
}

fn create_fixture_project() -> (PathBuf, PathBuf) {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after UNIX epoch")
        .as_nanos();
    let project = std::env::temp_dir().join(format!(
        "cargo-rebuild-analyzer-fixture-{}-{suffix}",
        std::process::id()
    ));
    fs::create_dir_all(project.join("src")).expect("failed to create fixture project");
    fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"fixture-project\"\nversion = \"0.1.0\"\nedition = \"2021\"\nbuild = \"build.rs\"\n",
    )
    .expect("failed to write fixture manifest");
    fs::write(
        project.join("build.rs"),
        "fn main() { println!(\"cargo:rerun-if-changed=build.rs\"); }\n",
    )
    .expect("failed to write fixture build script");
    fs::write(
        project.join("src/lib.rs"),
        "pub fn fixture_value() -> u32 { 1 }\n",
    )
    .expect("failed to write fixture source");

    (project.clone(), project.join("history.json"))
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
    assert!(stdout.contains("--json"));
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

#[test]
fn ci_json_output_contains_threshold_status() {
    let directory = std::env::temp_dir().join(format!(
        "cargo-rebuild-analyzer-json-ci-test-{}",
        std::process::id()
    ));
    fs::create_dir_all(&directory).expect("failed to create test history directory");
    let history = directory.join("history.json");

    let output = run_cli_with_history(&["--ci", "--json", "--max-rebuilds", "999"], &history);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let report: Value = serde_json::from_str(&stdout).expect("CI output should be valid JSON");

    assert!(output.status.success());
    assert_eq!(report["status"], "passed");
    assert_eq!(report["threshold_violation"], Value::Null);
    assert!(report.get("rebuilt").is_some());
    fs::remove_dir_all(directory).expect("failed to remove test history directory");
}

#[test]
fn real_build_scenarios_distinguish_initial_fresh_and_source_rebuilds() {
    let (project, history) = create_fixture_project();

    let initial = run_cli_in_project(&["--json"], &project, &history);
    assert!(initial.status.success());
    let initial_report: Value =
        serde_json::from_slice(&initial.stdout).expect("initial output should be valid JSON");
    assert!(initial_report["crates_rebuilt"].as_u64().unwrap_or(0) > 0);

    let unchanged = run_cli_in_project(&["--json"], &project, &history);
    assert!(unchanged.status.success());
    let unchanged_report: Value =
        serde_json::from_slice(&unchanged.stdout).expect("unchanged output should be valid JSON");
    assert_eq!(unchanged_report["crates_rebuilt"], 0);
    assert!(unchanged_report["crates_unchanged"].as_u64().unwrap_or(0) > 0);

    fs::write(
        project.join("src/lib.rs"),
        "pub fn fixture_value() -> u32 { 2 }\n",
    )
    .expect("failed to modify fixture source");

    let rebuilt = run_cli_in_project(&["--json"], &project, &history);
    assert!(rebuilt.status.success());
    let rebuilt_report: Value =
        serde_json::from_slice(&rebuilt.stdout).expect("rebuilt output should be valid JSON");
    assert!(rebuilt_report["crates_rebuilt"].as_u64().unwrap_or(0) > 0);
    assert!(rebuilt_report["estimated_wasted_ms"].as_u64().is_some());

    fs::write(
        project.join("build.rs"),
        "fn main() { println!(\"cargo:rerun-if-changed=build.rs\"); println!(\"cargo:rustc-env=FIXTURE_CHANGED=1\"); }\n",
    )
    .expect("failed to modify fixture build script");

    let build_script_rebuilt = run_cli_in_project(&["--json"], &project, &history);
    assert!(build_script_rebuilt.status.success());
    let build_script_report: Value = serde_json::from_slice(&build_script_rebuilt.stdout)
        .expect("build script output should be valid JSON");
    let rebuilt_build_script = build_script_report["rebuilt"]
        .as_array()
        .expect("rebuilt crates should be an array")
        .iter()
        .any(|crate_build| {
            crate_build["name"] == "fixture-project"
                && crate_build["target_kind"]
                    .as_str()
                    .is_some_and(|target| target.contains("build script"))
                && crate_build["likely_cause"] == "build script"
        });
    assert!(rebuilt_build_script);

    let history_contents = fs::read_to_string(&history).expect("history should be recorded");
    let history_json: Value =
        serde_json::from_str(&history_contents).expect("history should be valid JSON");
    assert_eq!(history_json["builds"].as_array().map(Vec::len), Some(4));

    fs::remove_dir_all(project).expect("failed to remove fixture project");
}
