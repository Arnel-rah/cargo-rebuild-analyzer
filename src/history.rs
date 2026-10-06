use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::models::{BuildHistory, BuildReport};

const HISTORY_FILE: &str = "target/rebuild-analyzer/history.json";

pub fn load() -> io::Result<BuildHistory> {
    let path = Path::new(HISTORY_FILE);
    if !path.exists() {
        return Ok(BuildHistory::default());
    }

    let contents = fs::read_to_string(path)?;
    serde_json::from_str(&contents).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid build history at {}: {error}", path.display()),
        )
    })
}

pub fn record(mut history: BuildHistory, report: BuildReport) -> io::Result<()> {
    let path = PathBuf::from(HISTORY_FILE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    history.builds.push(report);
    let contents = serde_json::to_string_pretty(&history)
        .map_err(|error| io::Error::other(format!("could not serialize build history: {error}")))?;
    fs::write(path, contents)
}
