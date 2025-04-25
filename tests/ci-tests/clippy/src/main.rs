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
//! Even if multiple Rust files are changed within the same crate, that crate is only linted once.
//!
//! # Usage
//! ```sh
//! cargo run --manifest-path tests/ci-tests/clippy/Cargo.toml
//! ```

use std::{
    collections::HashSet,
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

/// Runs Clippy on all crates that contain `.rs` files changed since the last shared commit
/// with `origin/main`.
///
/// # Behavior
/// 1. Uses `git` via subprocess to find the merge base between HEAD and `origin/main`.
/// 2. Diffs the two commits to find all changed `.rs` files.
/// 3. Walks up from each file to locate the nearest `Cargo.toml`.
/// 4. Uses `cargo_metadata` to find canonical crate names.
/// 5. Deduplicates affected crates and runs `cargo clippy` on each one.
///
/// If multiple files change within the same crate, that crate is only checked once.
///
/// # Errors
/// Returns an error if:
/// - Git commands fail or produce invalid output
/// - Metadata cannot be extracted from `cargo metadata`
/// - Any `cargo clippy` command fails
///
/// # Panics
/// This function does not intentionally panic.
///
/// # Examples
/// ```sh
/// cargo run --manifest-path tests/ci-tests/clippy/Cargo.toml
/// ```
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: Get the merge base between HEAD and origin/main
    let merge_base = Command::new("git")
        .args(["merge-base", "HEAD", "origin/main"])
        .output()?;

    if !merge_base.status.success() {
        return Err("Failed to get merge base from git.".into());
    }

    let merge_base_sha = String::from_utf8(merge_base.stdout)?.trim().to_string();

    // Step 2: Get list of changed Rust files since the merge base
    let diff_output = Command::new("git")
        .args(["diff", "--name-only", &format!("{merge_base_sha}...HEAD")])
        .output()?;

    if !diff_output.status.success() {
        return Err("Failed to get diff from git.".into());
    }

    let changed_rs_files: Vec<PathBuf> = String::from_utf8(diff_output.stdout)?
        .lines()
        .filter(|line| line.trim_end().ends_with(".rs"))
        .map(PathBuf::from)
        .collect();

    if changed_rs_files.is_empty() {
        println!("{}", colors::green("No changed Rust files found since origin/main."));
        return Ok(());
    }

    println!("Changed Rust files:");
    for f in &changed_rs_files {
        println!("  {}", f.display());
    }

    // Step 3: Walk up to find Cargo.toml for each changed file
    let mut crate_dirs = HashSet::new();
    for file in &changed_rs_files {
        let mut current = file.parent();
        while let Some(dir) = current {
            if dir.join("Cargo.toml").exists() {
                crate_dirs.insert(dir.to_path_buf());
                break;
            }
            current = dir.parent();
        }
    }

    if crate_dirs.is_empty() {
        println!("{}", colors::red("No crates found for changed files."));
        return Ok(());
    }

    // Step 4: Use cargo_metadata to resolve canonical crate names
    let metadata = cargo_metadata::MetadataCommand::new()
    .manifest_path("tests/ci-tests/clippy/Cargo.toml")
    .exec()?;
    let mut affected_crates = HashSet::new();

    for crate_dir in crate_dirs {
        for package in &metadata.packages {
            let manifest_path = Path::new(&package.manifest_path);
            if manifest_path.starts_with(&crate_dir) {
                affected_crates.insert(package.name.clone());
            }
        }
    }

    if affected_crates.is_empty() {
        println!("{}", colors::red("No crate names matched changed files."));
        println!("{}", colors::green("Running Clippy for current package instead."));
    
        let status = Command::new("cargo")
            .args([
                "clippy",
                "--manifest-path",
                "tests/ci-tests/clippy/Cargo.toml",
                "--all-targets",
                "--all-features",
                "--",
                "-D",
                "warnings",
            ])
            .status()?;
    
        if !status.success() {
            eprintln!("{}", colors::red("Clippy failed on fallback run."));
            std::process::exit(1);
        }
    
        println!("{}", colors::green("Fallback Clippy run passed successfully."));
        return Ok(());
    }

    // Step 5: Run Clippy for each affected crate
    for krate in &affected_crates {
        println!("Running Clippy for crate `{}`...", krate);
        let status = Command::new("cargo")
            .args([
                "clippy",
                "-p",
                krate,
                "--all-targets",
                "--all-features",
                "--",
                "-D",
                "warnings",
            ])
            .status()?;

        if !status.success() {
            eprintln!("{}", colors::red(&format!("Clippy failed on crate `{}`.", krate)));
            std::process::exit(1);
        }
    }

    println!("{}", colors::green("All Clippy checks passed successfully."));
    Ok(())
}
