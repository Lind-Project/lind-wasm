//! Clippy Delta Checker
//!
//! This tool detects which Rust files have changed since the last common commit with `origin/main`,
//! identifies which crates those files belong to, and runs `cargo clippy` on each affected crate.
//!
//! Intended to be used in CI or pre-push workflows to avoid running Clippy on the entire workspace.
//!
//! # Behavior
//! - All changed `.rs` files are collected via shell `git diff`.
//! - Each file is traced upward to the nearest `Cargo.toml`.
//! - The list of affected crates is deduplicated to ensure Clippy only runs once per crate.
//!
//! # Usage
//! ```sh
//! cargo run --manifest-path tests/ci-tests/clippy/Cargo.toml
//! ```

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Command,
    env,
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

    let output_file = args.iter()
        .position(|arg| arg == "--output-file")
        .and_then(|pos| args.get(pos + 1))
        .map(|s| s.clone())
        .unwrap_or_else(|| "tests/ci-tests/clippy/clippy_out.json".to_string());


    let diff_output = Command::new("git")
    .args(["diff", "--name-only", "HEAD^..HEAD", "--","*.rs"])
    .output()?;
    
    let changed_rs_files: Vec<PathBuf> = String::from_utf8(diff_output.stdout)?
        .lines()        
        .map(PathBuf::from)
        .collect();
    

    if changed_rs_files.is_empty() {
        println!("{}", colors::green("No changed Rust files found in HEAD commit."));
        output::write_results(&[], &output_file)?;
        return Ok(());
    }

    println!("Changed Rust files:");
    for f in &changed_rs_files {
        println!("  {}", f.display());
    }

    let mut manifest_paths = HashSet::new();

    for file in &changed_rs_files {
        if let Some(cargo_toml) = find_nearest_manifest(file) {
            manifest_paths.insert(cargo_toml);
        }
    }

    if manifest_paths.is_empty() {
        println!("{}", colors::red("No Cargo.toml files found for changed files."));
        output::write_results(&[], &output_file)?;
        return Ok(());
    }

    let mut results = Vec::new();
    let mut any_failed = false;

    for manifest_path in &manifest_paths {
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
            .output()?;

        if output.status.success() {
            results.push(output::RunResult {
                manifest_path: manifest_path.display().to_string(),
                status: "success".to_string(),
                error_body: None,
            });
            eprintln!("{}", colors::green(&format!("Clippy passed for manifest at `{}`.", manifest_path.display())));
        } else {
            any_failed = true;
            results.push(output::RunResult {
                manifest_path: manifest_path.display().to_string(),
                status: "failure".to_string(),
                error_body: Some(String::from_utf8_lossy(&output.stderr).to_string()),
            });
            eprintln!("{}", colors::red(&format!("Clippy failed for manifest at `{}`.", manifest_path.display())));
        }
    }

    output::write_results(&results, &output_file)?;

    if any_failed {
        std::process::exit(1);
    }

    println!("{}", colors::green("All Clippy checks passed."));
    Ok(())
}

/// Walk upward from a file to find the nearest Cargo.toml.
fn find_nearest_manifest(start: &Path) -> Option<PathBuf> {
    let mut current = start.parent();
    while let Some(dir) = current {
        let candidate = dir.join("Cargo.toml");
        if candidate.exists() {
            return Some(candidate);
        }
        current = dir.parent();
    }
    None
}
