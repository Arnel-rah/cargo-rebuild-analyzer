use std::io;
use std::process::{Command, Stdio};

use crate::models::BuildReport;
use crate::parser::parse_message;

pub fn run_build() -> io::Result<BuildReport> {
    let output = Command::new("cargo")
        .args(["build", "--message-format=json"])
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

    let compiled = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_message)
        .collect();

    Ok(BuildReport { compiled })
}
