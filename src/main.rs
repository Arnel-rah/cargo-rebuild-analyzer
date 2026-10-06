mod cargo;
mod models;
mod parser;

fn main() {
    if let Err(error) = cargo::run_build() {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}
