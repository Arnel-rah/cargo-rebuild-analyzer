mod cargo;
mod models;
mod parser;

fn main() {
    match cargo::run_build() {
        Ok(report) => {
            println!("Compiled {} crate(s):", report.compiled.len());
            for crate_build in report.compiled {
                println!("- {} {}", crate_build.name, crate_build.version);
            }
        }
        Err(error) => {
            eprintln!("Error: {error}");
            std::process::exit(1);
        }
    }
}
