use std::io;
use std::process::{Command, Stdio};

use crate::models::{BuildReport, CrateBuild};
use crate::parser::parse_message;

#[derive(Debug, Default)]
pub struct BuildOptions {
    pub release: bool,
    pub features: Vec<String>,
    pub package: Option<String>,
}

pub fn run_build(options: &BuildOptions) -> io::Result<BuildReport> {
    let mut command = Command::new("cargo");
    command.args(["build", "--message-format=json"]);

    if options.release {
        command.arg("--release");
    }

    if !options.features.is_empty() {
        command.args(["--features", &options.features.join(",")]);
    }

    if let Some(package) = &options.package {
        command.args(["--package", package]);
    }

    let output = command
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()?;

    if !output.status.success() {
        return Err(io::Error::other(format!(
            "cargo build failed with status: {}",
            output.status
        )));
    }

    let compiled: Vec<CrateBuild> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_message)
        .collect();

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| io::Error::other(format!("system clock error: {error}")))?
        .as_secs();

    Ok(BuildReport {
        timestamp,
        release: options.release,
        features: options.features.clone(),
        package: options.package.clone(),
        compiled,
    })
}
