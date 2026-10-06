use std::io;
use std::process::{Command, Stdio};

pub fn run_build() -> io::Result<()> {
    let status = Command::new("cargo")
        .args(["build", "--message-format=json"])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    if !status.success() {
        return Err(io::Error::other(format!(
            "cargo build failed with status: {}",
            status
        )));
    }

    Ok(())
}
