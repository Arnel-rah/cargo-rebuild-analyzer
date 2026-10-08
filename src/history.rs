use std::fs;
use std::io;
use std::path::PathBuf;

use crate::models::{BuildHistory, BuildReport};

const DEFAULT_HISTORY_FILE: &str = "target/rebuild-analyzer/history.json";

fn history_file() -> PathBuf {
    std::env::var_os("CARGO_REBUILD_ANALYZER_HISTORY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_HISTORY_FILE))
}

pub fn load() -> io::Result<BuildHistory> {
    let path = history_file();
    if !path.exists() {
        return Ok(BuildHistory::default());
    }

    let contents = fs::read_to_string(&path)?;
    serde_json::from_str(&contents).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid build history at {}: {error}", path.display()),
        )
    })
}

pub fn record(mut history: BuildHistory, report: BuildReport) -> io::Result<()> {
    let path = history_file();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    history.builds.push(report);
    let contents = serde_json::to_string_pretty(&history)
        .map_err(|error| io::Error::other(format!("could not serialize build history: {error}")))?;
    fs::write(path, contents)
}

pub fn latest_for<'a>(history: &'a BuildHistory, report: &BuildReport) -> Option<&'a BuildReport> {
    history
        .builds
        .iter()
        .rev()
        .find(|build| same_configuration(build, report))
}

fn same_configuration(left: &BuildReport, right: &BuildReport) -> bool {
    left.release == right.release
        && left.features == right.features
        && left.package == right.package
}
