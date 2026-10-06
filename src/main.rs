use clap::Parser;

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
    /// Build in release mode.
    #[arg(long)]
    release: bool,

    /// Enable one or more Cargo features (comma-separated or repeated).
    #[arg(long, value_delimiter = ',')]
    features: Vec<String>,

    /// Build only the selected package.
    #[arg(short = 'p', long)]
    package: Option<String>,
}

fn main() {
    let cli = Cli::parse();
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
                let previous_count = previous
                    .builds
                    .last()
                    .map_or(0, |build| build.compiled.len());

                println!("Cargo Rebuild Analyzer");
                println!();
                println!("Crates analyzed: {}", report.compiled.len());
                println!("Crates rebuilt: {rebuilt}");
                println!("Crates unchanged: {fresh}");
                if previous_count > 0 {
                    println!("Previous build: {previous_count} crate(s) analyzed");
                }
                println!();

                for crate_build in report.compiled.iter().filter(|build| !build.fresh) {
                    println!("⚠ {} {}", crate_build.name, crate_build.version);
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
