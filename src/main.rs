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

    Why { crate_name: String },
}

fn main() {
    let cli = Cli::parse();

    if let Some(command) = cli.command {
        let result = match command {
            Commands::History => show_history(),
            Commands::Why { crate_name } => show_why(&crate_name),
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

    Ok(())
}
