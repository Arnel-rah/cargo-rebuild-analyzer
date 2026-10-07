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

pub fn latest_for<'a>(history: &'a BuildHistory, report: &BuildReport) -> Option<&'a BuildReport> {
    history
        .builds
        .iter()
        .rev()
        .find(|build| same_configuration(build, report))
}

pub fn same_configuration(left: &BuildReport, right: &BuildReport) -> bool {
    left.release == right.release
        && left.features == right.features
        && left.package == right.package
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(release: bool, features: &[&str], package: Option<&str>) -> BuildReport {
        BuildReport {
            timestamp: 0,
            release,
            features: features
                .iter()
                .map(|feature| (*feature).to_string())
                .collect(),
            package: package.map(str::to_string),
            compiled: Vec::new(),
        }
    }

    #[test]
    fn matches_only_the_same_build_configuration() {
        let debug = report(false, &[], None);
        let release = report(true, &[], None);
        let feature_build = report(false, &["logging"], None);

        assert!(same_configuration(&debug, &debug));
        assert!(!same_configuration(&debug, &release));
        assert!(!same_configuration(&debug, &feature_build));
    }

    #[test]
    fn finds_the_latest_matching_build() {
        let requested = report(true, &["logging"], Some("app"));
        let history = BuildHistory {
            builds: vec![
                report(false, &[], None),
                report(true, &["logging"], Some("app")),
                report(true, &[], None),
            ],
        };

        assert_eq!(
            latest_for(&history, &requested)
                .expect("matching build should exist")
                .features,
            vec!["logging"]
        );
    }
}
