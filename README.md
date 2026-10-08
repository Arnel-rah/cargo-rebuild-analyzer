# Cargo Rebuild Analyzer

> Explain why Cargo dependencies are rebuilt and identify rebuilds that may be
> expensive or unnecessary.

Cargo knows how to compile a Rust project, but its build output does not keep a
long-term explanation of what was rebuilt. Cargo Rebuild Analyzer adds a
diagnostic layer above Cargo: it captures Cargo's JSON build messages, detects
fresh and rebuilt crates, infers a likely cause, and stores a local build
history.

## Current status

The project currently provides the MVP:

- captures `cargo build --message-format=json` output;
- detects `fresh` versus rebuilt compiler artifacts;
- identifies likely causes such as build scripts and native compilation;
- records build reports in `target/rebuild-analyzer/history.json`;
- compares a build with the latest matching profile, feature set, and package;
- exposes `history` and `why <crate>` commands;
- validates package and feature names before starting a build;
- measures total build duration and estimates the portion associated with
  rebuilt crates.
- stores Cargo's resolved dependency graph and displays a dependency path for
  `why <crate>` when available.
- provides a detailed `report` command with human-readable and JSON output.
- refines likely rebuild causes using the previous matching build and rebuilt
  dependency relationships.

## Requirements

- Rust and Cargo with edition 2024 support.
- A Cargo project to analyze.

## Installation

Clone the repository and build the binary:

```bash
git clone <repository-url>
cd cargo-rebuild-analyzer
cargo build --release
```

The executable is then available at:

```text
target/release/cargo-rebuild-analyzer
```

For local development, use `cargo run` from the project you want to analyze.

## Usage

Run a standard debug build:

```bash
cargo run
```

Run a release build:

```bash
cargo run -- --release
```

Enable Cargo features:

```bash
cargo run -- --features feature_a,feature_b
```

Features must be declared by the analyzed project. Multiple `--features`
arguments are also accepted.

Build one package from a workspace:

```bash
cargo run -- --package my-package
```

Invalid package and feature names are rejected before Cargo starts compiling,
with a list of available values.

## CI thresholds

Use `--ci` to make the build command enforce rebuild budgets. The build is
still recorded in the normal history, but the process exits with status `2`
when a configured threshold is exceeded:

```bash
cargo run -- --ci --max-rebuilds 5
cargo run -- --ci --max-wasted-time-ms 1000
cargo run -- --ci --max-rebuilds 5 --max-wasted-time-ms 1000
cargo run -- --ci --json --max-rebuilds 5
```

The limits are optional, but they require `--ci`. A successful check exits
with status `0`; invalid input or a Cargo failure exits with status `1`; a
threshold violation exits with status `2`.

Add `--json` to emit one machine-readable build summary containing the CI
status, thresholds result, counts, duration, and rebuilt crates:

```bash
cargo run -- --ci --json --max-rebuilds 5 > rebuild-report.json
```

For GitHub Actions, fail a job when a build rebuilds more than five crates:

```yaml
- name: Check rebuild budget
  run: cargo run --release -- --ci --max-rebuilds 5
```

This repository includes a complete workflow at
`.github/workflows/ci.yml`. It checks formatting, tests, Clippy, and runs the
analyzer with a permissive rebuild budget. The workflow also publishes the
JSON results as a summary in the GitHub Actions interface. Projects adopting
the analyzer can replace `999` with their own rebuild limit.

## Inspecting the history

Display all recorded builds:

```bash
cargo run -- history
```

Each record includes its timestamp, profile, package, features, number of
analyzed crates, and number of rebuilt crates.

The history command also displays cumulative statistics per rebuilt crate:

```bash
cargo run -- history
cargo run -- history --crate serde_json
```

Statistics include rebuild count, estimated associated time, last rebuild
timestamp, and the likely causes observed across builds.

Explain why a crate was rebuilt:

```bash
cargo run -- why serde_json
```

The command reports how many recorded rebuilds were found and the likely
causes. It also displays the dependency chain from the analyzed package to the
crate when Cargo's resolved graph contains a path:

```text
serde_json
  Rebuilt 2 time(s)
  Likely cause: source or dependency change
  Target: library
  Dependency chain: cargo-rebuild-analyzer -> serde_json
```

An error is returned when no rebuild for the requested crate exists in the
history. History records created before dependency graph support remain
readable, but do not contain a dependency chain.

## Detailed reports

Display a report for the latest recorded build:

```bash
cargo run -- report
```

Export the same report as JSON for CI or other tooling:

```bash
cargo run -- report --json
```

The report includes the profile, features, package selection, analyzed and
rebuilt crate counts, build duration, estimated wasted time, and the list of
rebuilt crates with their target type and likely cause.

## Configuration-aware comparisons

Debug and release builds are tracked separately. Feature sets and package
selection are also part of a build configuration, so an incompatible build is
not used as the previous comparison.

Feature names are sorted and deduplicated before they are recorded. Therefore,
these commands use the same configuration:

```bash
cargo run -- --features logging,database
cargo run -- --features database,logging
```

## Example output

```text
Cargo Rebuild Analyzer

Crates analyzed: 30
Crates rebuilt: 2
Crates unchanged: 28
Previous matching build: 30 crate(s) analyzed
Build duration: 12.34 s
Estimated rebuild time: 0.82 s

⚠ openssl-sys 0.9.110
  Target: build script
  Likely cause: native compilation
```

The estimated rebuild time is a proportional estimate based on the ratio of
rebuilt artifacts to analyzed artifacts. Cargo's JSON stream does not expose a
reliable duration for each individual `compiler-artifact`, so this value is an
indicator rather than a per-crate profiler.

Likely causes currently include:

- `build script`;
- `native compilation`;
- `dependency rebuilt`;
- `source or dependency change`;
- `initial build or configuration change`.

Cargo build-script targets are associated with their package instead of being
reported only as the generic `build-script-build` target. Existing history
created before this field was added remains readable and displays
`Target: unknown` for those older records.

## History storage

Build history is stored locally at:

```text
target/rebuild-analyzer/history.json
```

The `target/` directory is generated build data and should not be committed.
Remove the history file when you want to start a fresh analysis.

## Development

Run the project checks:

```bash
cargo fmt -- --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

## Roadmap

- Add build duration tracking and estimated wasted compilation time.
- Improve crate identity so build-script targets are associated with their
  package instead of appearing as duplicate `build-script-build` entries.
- Add dependency-chain analysis for `why <crate>`.
- Add a `report` command with JSON and human-readable output.
- Add richer historical statistics for frequently rebuilt crates.

## License

This project does not currently declare a license.
