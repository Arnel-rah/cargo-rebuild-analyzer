use std::collections::BTreeMap;
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

    /// Return a non-zero status when a configured rebuild threshold is exceeded.
    #[arg(long)]
    ci: bool,

    /// Print the build result as machine-readable JSON.
    #[arg(long)]
    json: bool,

    /// Maximum allowed rebuilt crates in CI mode.
    #[arg(long, requires = "ci")]
    max_rebuilds: Option<usize>,

    /// Maximum allowed estimated wasted build time in milliseconds in CI mode.
    #[arg(long, requires = "ci")]
    max_wasted_time_ms: Option<u64>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Show recorded builds and per-crate rebuild statistics.
    History {
        /// Limit statistics to one crate.
        #[arg(long = "crate")]
        crate_name: Option<String>,
    },

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
            Commands::History { crate_name } => show_history(crate_name.as_deref()),
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

                if !cli.json {
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
                }

                let estimated_wasted_ms = report.estimated_wasted_ms;
                let violation = if cli.ci {
                    threshold_violation(
                        rebuilt,
                        estimated_wasted_ms,
                        cli.max_rebuilds,
                        cli.max_wasted_time_ms,
                    )
                } else {
                    None
                };
                if cli.json
                    && let Err(error) = print_build_json(
                        &report,
                        rebuilt,
                        fresh,
                        previous_count,
                        violation.as_deref(),
                    )
                {
                    eprintln!("Error serializing build report: {error}");
                    std::process::exit(1);
                }
                if let Err(error) = history::record(previous, report) {
                    eprintln!("Error recording build history: {error}");
                    std::process::exit(1);
                }

                if let Some(message) = violation {
                    eprintln!("CI threshold exceeded: {message}");
                    std::process::exit(2);
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

fn print_build_json(
    report: &models::BuildReport,
    rebuilt: usize,
    fresh: usize,
    previous_count: usize,
    violation: Option<&str>,
) -> std::io::Result<()> {
    let payload = serde_json::json!({
        "status": if violation.is_some() { "threshold_exceeded" } else { "passed" },
        "profile": if report.release { "release" } else { "debug" },
        "features": report.features,
        "package": report.package,
        "crates_analyzed": report.compiled.len(),
        "crates_rebuilt": rebuilt,
        "crates_unchanged": fresh,
        "duration_ms": report.duration_ms,
        "estimated_wasted_ms": report.estimated_wasted_ms,
        "previous_matching_crates": previous_count,
        "threshold_violation": violation,
        "rebuilt": report.compiled.iter().filter(|build| !build.fresh).collect::<Vec<_>>(),
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&payload).map_err(|error| std::io::Error::other(format!(
            "could not serialize report: {error}"
        )))?
    );
    Ok(())
}

fn threshold_violation(
    rebuilt: usize,
    estimated_wasted_ms: u64,
    max_rebuilds: Option<usize>,
    max_wasted_time_ms: Option<u64>,
) -> Option<String> {
    if let Some(limit) = max_rebuilds
        && rebuilt > limit
    {
        return Some(format!("{rebuilt} rebuilt crates (limit: {limit})"));
    }

    if let Some(limit) = max_wasted_time_ms
        && estimated_wasted_ms > limit
    {
        return Some(format!(
            "{} estimated wasted time (limit: {} ms)",
            format_duration(estimated_wasted_ms),
            limit
        ));
    }

    None
}

fn show_history(crate_filter: Option<&str>) -> std::io::Result<()> {
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

    let statistics = crate_statistics(&history, crate_filter);
    if !statistics.is_empty() {
        println!();
        println!("Crate rebuild statistics");
        for (name, stats) in statistics {
            println!("{}:", name);
            println!("  Rebuilt: {} time(s)", stats.rebuilds);
            println!(
                "  Estimated time: {}",
                format_duration(stats.estimated_wasted_ms)
            );
            println!("  Last rebuild: {}", stats.last_timestamp);
            println!("  Likely causes: {}", stats.causes.join(", "));
        }
    }

    Ok(())
}

#[derive(Default)]
struct CrateStatistics {
    rebuilds: usize,
    estimated_wasted_ms: u64,
    last_timestamp: u64,
    causes: Vec<String>,
}

fn crate_statistics(
    history: &models::BuildHistory,
    crate_filter: Option<&str>,
) -> BTreeMap<String, CrateStatistics> {
    let mut statistics = BTreeMap::new();

    for build in &history.builds {
        let rebuilt_count = build
            .compiled
            .iter()
            .filter(|crate_build| !crate_build.fresh)
            .count() as u64;
        let per_crate_estimate = build
            .estimated_wasted_ms
            .checked_div(rebuilt_count)
            .unwrap_or(0);

        for crate_build in build
            .compiled
            .iter()
            .filter(|crate_build| !crate_build.fresh)
        {
            if crate_filter.is_some_and(|filter| filter != crate_build.name) {
                continue;
            }

            let stats = statistics
                .entry(crate_build.name.clone())
                .or_insert_with(CrateStatistics::default);
            stats.rebuilds += 1;
            stats.estimated_wasted_ms =
                stats.estimated_wasted_ms.saturating_add(per_crate_estimate);
            stats.last_timestamp = stats.last_timestamp.max(build.timestamp);
            if !stats.causes.contains(&crate_build.likely_cause) {
                stats.causes.push(crate_build.likely_cause.clone());
                stats.causes.sort();
            }
        }
    }

    statistics
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

#[cfg(test)]
mod tests {
    use super::threshold_violation;

    #[test]
    fn reports_rebuild_threshold_violations() {
        assert_eq!(
            threshold_violation(3, 0, Some(2), None),
            Some("3 rebuilt crates (limit: 2)".to_string())
        );
        assert_eq!(threshold_violation(2, 0, Some(2), None), None);
    }

    #[test]
    fn reports_wasted_time_threshold_violations() {
        assert_eq!(
            threshold_violation(0, 1_500, None, Some(1_000)),
            Some("1.50 s estimated wasted time (limit: 1000 ms)".to_string())
        );
    }
}
