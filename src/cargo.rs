use std::io;
use std::process::{Command, Stdio};
use std::time::Instant;

use crate::models::{BuildReport, CrateBuild, DependencyNode};
use crate::parser::parse_message;

#[derive(Debug, Default)]
pub struct BuildOptions {
    pub release: bool,
    pub features: Vec<String>,
    pub package: Option<String>,
}

pub fn run_build(options: &BuildOptions) -> io::Result<BuildReport> {
    validate_options(options)?;

    let mut features = options.features.clone();
    features.sort();
    features.dedup();

    let mut command = Command::new("cargo");
    command.args(["build", "--message-format=json"]);

    if options.release {
        command.arg("--release");
    }

    if !features.is_empty() {
        command.args(["--features", &features.join(",")]);
    }

    if let Some(package) = &options.package {
        command.args(["--package", package]);
    }

    let build_started = Instant::now();
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
    let duration_ms = build_started.elapsed().as_millis() as u64;
    let rebuilt_count = compiled.iter().filter(|build| !build.fresh).count() as u64;
    let compiled_count = compiled.len() as u64;
    let estimated_wasted_ms = duration_ms
        .saturating_mul(rebuilt_count)
        .checked_div(compiled_count)
        .unwrap_or(0);

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| io::Error::other(format!("system clock error: {error}")))?
        .as_secs();
    let (root_package, dependencies) = dependency_graph(options.package.as_deref())?;

    Ok(BuildReport {
        timestamp,
        duration_ms,
        estimated_wasted_ms,
        release: options.release,
        features,
        package: options.package.clone(),
        root_package,
        dependencies,
        compiled,
    })
}

fn dependency_graph(
    selected_package: Option<&str>,
) -> io::Result<(Option<String>, Vec<DependencyNode>)> {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1"])
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
    let packages = metadata["packages"].as_array().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "cargo metadata has no packages")
    })?;
    let names: std::collections::HashMap<_, _> = packages
        .iter()
        .filter_map(|package| {
            Some((
                package["id"].as_str()?.to_string(),
                package["name"].as_str()?.to_string(),
            ))
        })
        .collect();
    let root_id = selected_package
        .and_then(|name| packages.iter().find(|package| package["name"] == name))
        .and_then(|package| package["id"].as_str().map(str::to_string))
        .or_else(|| metadata["resolve"]["root"].as_str().map(str::to_string));
    let nodes = metadata["resolve"]["nodes"].as_array().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "cargo metadata has no resolve graph",
        )
    })?;
    let graph = nodes
        .iter()
        .filter_map(|node| {
            let package_id = node["id"].as_str()?;
            let package = names.get(package_id)?.clone();
            let dependencies = node["dependencies"]
                .as_array()?
                .iter()
                .filter_map(|dependency| names.get(dependency.as_str()?).cloned())
                .collect();
            Some(DependencyNode {
                package,
                dependencies,
            })
        })
        .collect();
    Ok((root_id.and_then(|id| names.get(&id).cloned()), graph))
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
