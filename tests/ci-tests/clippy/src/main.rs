//! Clippy Delta Checker
//!
//! This tool detects all Rust crates in the current workspace
//! and runs `cargo clippy` on each manifest in parallel.
//!
//! Intended for CI or pre-push workflows to enforce lint hygiene across multiple crates.
//!
//! # Behavior
//! - Finds all `Cargo.toml` files recursively from the root.
//! - Runs `cargo clippy` in parallel (Rayon) for each crate.
//! - Writes a JSON and HTML report to the specified output path.
//!
//! # Usage
//! ```sh
//! cargo run --manifest-path tests/ci-tests/clippy/Cargo.toml
//! ```
use rayon::prelude::*;
use std::{
    collections::HashSet,
    env,
    path::{Path, PathBuf},
    process::Command,
};

/// Simple ANSI escape codes and color logic for output.
mod colors {
    pub const RED: &str = "\x1b[31m";
    pub const GREEN: &str = "\x1b[32m";
    pub const RESET: &str = "\x1b[0m";

    /// Returns a red-colored string if outputting to a TTY.
    pub fn red(text: &str) -> String {
        if is_tty() {
            format!("{RED}{text}{RESET}")
        } else {
            text.to_string()
        }
    }

    /// Returns a green-colored string if outputting to a TTY.
    pub fn green(text: &str) -> String {
        if is_tty() {
            format!("{GREEN}{text}{RESET}")
        } else {
            text.to_string()
        }
    }

    fn is_tty() -> bool {
        atty::is(atty::Stream::Stdout)
    }
}

mod output;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    // Determine the output path for the JSON/HTML report
    let output_file = args.iter()
        .position(|arg| arg == "--output-file")
        .and_then(|pos| args.get(pos + 1))
        .cloned()
        .unwrap_or_else(|| "tests/ci-tests/clippy/clippy_out.json".to_string());
    // Find all crate manifests in the workspace
    let manifest_paths: HashSet<PathBuf> = find_all_manifests(Path::new("."))?;

    if manifest_paths.is_empty() {
        println!("{}", colors::red("No Cargo.toml files found in workspace."));
        output::write_results(&[], &output_file)?;
        return Ok(());
    }

    println!("Found the following crate manifests:");
    for path in &manifest_paths {
        println!("  {}", path.display());
    }

    // Run Clippy on each manifest in parallel
    let results: Vec<_> = manifest_paths
        .par_iter()
        .map(|manifest_path| {
            println!("Running Clippy for manifest at `{}`...", manifest_path.display());

            let output = Command::new("cargo")
                .args([
                    "clippy",
                    "--manifest-path",
                    &manifest_path.to_string_lossy(),
                    "--all-targets",
                    "--all-features",
                    "--",
                    "-D",
                    "warnings",
                ])
                .output();

            match output {
                Ok(out) if out.status.success() => {
                    eprintln!("{}", colors::green(&format!("Clippy passed for `{}`.", manifest_path.display())));
                    output::RunResult {
                        manifest_path: manifest_path.display().to_string(),
                        status: "success".to_string(),
                        error_body: None,
                    }
                }
                Ok(out) => {
                    eprintln!("{}", colors::red(&format!("Clippy failed for `{}`.", manifest_path.display())));
                    output::RunResult {
                        manifest_path: manifest_path.display().to_string(),
                        status: "failure".to_string(),
                        error_body: Some(String::from_utf8_lossy(&out.stderr).to_string()),
                    }
                }
                Err(e) => {
                    eprintln!("{}", colors::red(&format!("Error running Clippy for `{}`: {e}", manifest_path.display())));
                    output::RunResult {
                        manifest_path: manifest_path.display().to_string(),
                        status: "error".to_string(),
                        error_body: Some(e.to_string()),
                    }
                }
            }
        })
        .collect();

    let any_failed = results.iter().any(|r| r.status != "success");

    output::write_results(&results, &output_file)?;

    if any_failed {
        std::process::exit(1);
    }

    println!("{}", colors::green("All Clippy checks passed."));
    Ok(())
}

/// Recursively walks the directory tree to find all Cargo.toml files.
///
/// # Arguments
/// * `start_dir` - Path to begin the directory search.
///
/// # Returns
/// A `HashSet` of manifest paths (one for each discovered crate).
fn find_all_manifests(start_dir: &Path) -> Result<HashSet<PathBuf>, Box<dyn std::error::Error>> {
    let mut manifests = HashSet::new();
    let mut dirs = vec![start_dir.to_path_buf()];

    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                dirs.push(path);
            } else if path.file_name().map_or(false, |f| f == "Cargo.toml") {
                manifests.insert(path);
            }
        }
    }

    Ok(manifests)
}

