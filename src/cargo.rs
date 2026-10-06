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
    validate_options(options)?;

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

fn validate_options(options: &BuildOptions) -> io::Result<()> {
    let output = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()?;

    if !output.status.success() {
        return Err(io::Error::other(format!(
            "cargo metadata failed with status: {}",
            output.status
        )));
    }

    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("could not parse cargo metadata: {error}"),
        )
    })?;
    let packages = metadata
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "cargo metadata has no packages")
        })?;

    let package = options
        .package
        .as_deref()
        .map(|name| {
            packages
                .iter()
                .find(|package| {
                    package.get("name").and_then(serde_json::Value::as_str) == Some(name)
                })
                .ok_or_else(|| {
                    let available = package_names(packages);
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!(
                            "unknown package '{name}'. Available packages: {}",
                            available.join(", ")
                        ),
                    )
                })
        })
        .transpose()?;

    if !options.features.is_empty() {
        let feature_package = package.or_else(|| packages.first());
        let available = feature_package
            .and_then(|package| package.get("features"))
            .and_then(serde_json::Value::as_object)
            .map(|features| features.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        let invalid: Vec<_> = options
            .features
            .iter()
            .filter(|feature| !available.iter().any(|candidate| candidate == *feature))
            .collect();

        if !invalid.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "unknown feature(s): {}. Available features: {}",
                    invalid
                        .iter()
                        .map(|feature| feature.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    if available.is_empty() {
                        "none".to_string()
                    } else {
                        available.join(", ")
                    }
                ),
            ));
        }
    }

    Ok(())
}

fn package_names(packages: &[serde_json::Value]) -> Vec<String> {
    packages
        .iter()
        .filter_map(|package| {
            package
                .get("name")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .collect()
}
