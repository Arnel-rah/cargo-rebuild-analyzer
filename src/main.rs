use std::collections::HashSet;

use clap::{Parser, Subcommand};

mod cargo;
mod history;
mod models;
mod parser;

#[derive(Debug, Parser)]
#[command(
    name = "cargo-rebuild-analyzer",
    about = "Build a Cargo project and report the compiled crates"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[arg(long)]
    release: bool,

    #[arg(long, value_delimiter = ',')]
    features: Vec<String>,

    #[arg(short = 'p', long)]
    package: Option<String>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    History,

    Why {
        crate_name: String,
    },

    /// Show a detailed report for the latest recorded build.
    Report {
        /// Output the report as JSON.
        #[arg(long)]
        json: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    if let Some(command) = cli.command {
        let result = match command {
            Commands::History => show_history(),
            Commands::Why { crate_name } => show_why(&crate_name),
            Commands::Report { json } => show_report(json),
        };

        if let Err(error) = result {
            eprintln!("Error: {error}");
            std::process::exit(1);
        }
        return;
    }

    let options = cargo::BuildOptions {
        release: cli.release,
        features: cli.features,
        package: cli.package,
    };

    match cargo::run_build(&options) {
        Ok(report) => match history::load() {
            Ok(previous) => {
                let mut report = report;
                let previous_matching = history::latest_for(&previous, &report);
                refine_rebuild_causes(&mut report, previous_matching);
                let rebuilt = report.compiled.iter().filter(|build| !build.fresh).count();
                let fresh = report.compiled.iter().filter(|build| build.fresh).count();
                let previous_count =
                    history::latest_for(&previous, &report).map_or(0, |build| build.compiled.len());

                println!("Cargo Rebuild Analyzer");
                println!();
                println!("Crates analyzed: {}", report.compiled.len());
                println!("Crates rebuilt: {rebuilt}");
                println!("Crates unchanged: {fresh}");
                println!("Build duration: {}", format_duration(report.duration_ms));
                println!(
                    "Estimated rebuild time: {}",
                    format_duration(report.estimated_wasted_ms)
                );
                if previous_count > 0 {
                    println!("Previous matching build: {previous_count} crate(s) analyzed");
                }
                println!();

                for crate_build in report.compiled.iter().filter(|build| !build.fresh) {
                    println!("⚠ {} {}", crate_build.name, crate_build.version);
                    println!("  Target: {}", crate_build.target_kind);
                    println!("  Likely cause: {}", crate_build.likely_cause);
                }

                if let Err(error) = history::record(previous, report) {
                    eprintln!("Error recording build history: {error}");
                    std::process::exit(1);
                }
            }
            Err(error) => {
                eprintln!("Error reading build history: {error}");
                std::process::exit(1);
            }
        },
        Err(error) => {
            eprintln!("Error: {error}");
            std::process::exit(1);
        }
    }
}

fn show_history() -> std::io::Result<()> {
    let history = history::load()?;

    println!("Cargo Rebuild Analyzer history");
    if history.builds.is_empty() {
        println!("No builds recorded.");
        return Ok(());
    }

    for (index, build) in history.builds.iter().enumerate() {
        let rebuilt = build
            .compiled
            .iter()
            .filter(|crate_build| !crate_build.fresh)
            .count();
        println!(
            "{}. timestamp={} profile={} crates={} rebuilt={}{}",
            index + 1,
            build.timestamp,
            if build.release { "release" } else { "debug" },
            build.compiled.len(),
            rebuilt,
            build
                .package
                .as_deref()
                .map_or(String::new(), |package| format!(" package={package}"))
        );
        println!(
            "   duration={} estimated_rebuild_time={}",
            format_duration(build.duration_ms),
            format_duration(build.estimated_wasted_ms)
        );
        println!(
            "   features={}",
            if build.features.is_empty() {
                "none".to_string()
            } else {
                build.features.join(",")
            }
        );
    }

    Ok(())
}

fn show_report(json: bool) -> std::io::Result<()> {
    let history = history::load()?;
    let build = history.builds.last().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no build recorded; run the analyzer first",
        )
    })?;
    let rebuilt: Vec<_> = build
        .compiled
        .iter()
        .filter(|crate_build| !crate_build.fresh)
        .collect();
    let fresh = build.compiled.len().saturating_sub(rebuilt.len());

    if json {
        let report = serde_json::json!({
            "timestamp": build.timestamp,
            "profile": if build.release { "release" } else { "debug" },
            "features": build.features,
            "package": build.package,
            "crates_analyzed": build.compiled.len(),
            "crates_rebuilt": rebuilt.len(),
            "crates_unchanged": fresh,
            "duration_ms": build.duration_ms,
            "estimated_wasted_ms": build.estimated_wasted_ms,
            "rebuilt": rebuilt,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|error| {
                std::io::Error::other(format!("could not serialize report: {error}"))
            })?
        );
        return Ok(());
    }

    println!("Cargo Rebuild Analyzer report");
    println!();
    println!(
        "Profile: {}",
        if build.release { "release" } else { "debug" }
    );
    println!("Crates analyzed: {}", build.compiled.len());
    println!("Crates rebuilt: {}", rebuilt.len());
    println!("Crates unchanged: {fresh}");
    println!("Build duration: {}", format_duration(build.duration_ms));
    println!(
        "Estimated wasted time: {}",
        format_duration(build.estimated_wasted_ms)
    );
    if let Some(package) = &build.package {
        println!("Package: {package}");
    }
    if !build.features.is_empty() {
        println!("Features: {}", build.features.join(", "));
    }
    if !rebuilt.is_empty() {
        println!();
        println!("Rebuilt crates:");
        for crate_build in rebuilt {
            println!(
                "- {} {} ({}, {})",
                crate_build.name,
                crate_build.version,
                crate_build.target_kind,
                crate_build.likely_cause
            );
        }
    }

    Ok(())
}

fn refine_rebuild_causes(report: &mut models::BuildReport, previous: Option<&models::BuildReport>) {
    let rebuilt_names: HashSet<String> = report
        .compiled
        .iter()
        .filter(|build| !build.fresh)
        .map(|build| build.name.clone())
        .collect();
    let previous_crates = previous
        .map(|build| build.compiled.as_slice())
        .unwrap_or_default();
    let dependencies = report.dependencies.clone();

    for crate_build in report.compiled.iter_mut().filter(|build| !build.fresh) {
        if crate_build.target_kind.contains("build script")
            || crate_build.likely_cause == "native compilation"
        {
            continue;
        }

        let dependency_rebuilt = dependencies.iter().any(|node| {
            node.dependencies
                .iter()
                .any(|dependency| dependency == &crate_build.name)
                && rebuilt_names.contains(&node.package)
        });

        crate_build.likely_cause = if dependency_rebuilt {
            "dependency rebuilt".to_string()
        } else if previous_crates
            .iter()
            .any(|build| build.name == crate_build.name && build.fresh)
        {
            "source or dependency change".to_string()
        } else if previous.is_none() {
            "initial build or configuration change".to_string()
        } else {
            "source or dependency change".to_string()
        };
    }
}

fn format_duration(milliseconds: u64) -> String {
    if milliseconds < 1_000 {
        return format!("{milliseconds} ms");
    }

    let seconds = milliseconds / 1_000;
    let remaining_milliseconds = milliseconds % 1_000;
    if seconds < 60 {
        return format!("{seconds}.{:02} s", remaining_milliseconds / 10);
    }

    format!(
        "{}m {}.{:02}s",
        seconds / 60,
        seconds % 60,
        remaining_milliseconds / 10
    )
}

fn show_why(crate_name: &str) -> std::io::Result<()> {
    let history = history::load()?;
    let observations: Vec<_> = history
        .builds
        .iter()
        .flat_map(|build| build.compiled.iter())
        .filter(|crate_build| crate_build.name == crate_name && !crate_build.fresh)
        .collect();

    if observations.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no rebuild recorded for crate '{crate_name}'"),
        ));
    }

    println!("{crate_name}");
    println!("  Rebuilt {} time(s)", observations.len());
    let mut causes: Vec<&str> = observations
        .iter()
        .map(|crate_build| crate_build.likely_cause.as_str())
        .collect();
    causes.sort_unstable();
    causes.dedup();
    println!("  Likely cause: {}", causes.join(", "));
    let mut targets: Vec<&str> = observations
        .iter()
        .map(|crate_build| crate_build.target_kind.as_str())
        .collect();
    targets.sort_unstable();
    targets.dedup();
    println!("  Target: {}", targets.join(", "));
    if let Some(build) = history
        .builds
        .iter()
        .rev()
        .find(|build| build.compiled.iter().any(|item| item.name == crate_name))
        && let Some(path) = dependency_path(build, crate_name)
    {
        println!("  Dependency chain: {}", path.join(" -> "));
    }

    Ok(())
}

fn dependency_path(report: &models::BuildReport, target: &str) -> Option<Vec<String>> {
    let root = report.root_package.as_ref()?;
    let mut path = Vec::new();
    find_dependency_path(&report.dependencies, root, target, &mut path).then_some(path)
}

fn find_dependency_path(
    graph: &[models::DependencyNode],
    current: &str,
    target: &str,
    path: &mut Vec<String>,
) -> bool {
    path.push(current.to_string());
    if current == target {
        return true;
    }
    let dependencies = graph
        .iter()
        .find(|node| node.package == current)
        .map(|node| node.dependencies.as_slice())
        .unwrap_or_default();
    for dependency in dependencies {
        if !path.contains(dependency) && find_dependency_path(graph, dependency, target, path) {
            return true;
        }
    }
    path.pop();
    false
}
