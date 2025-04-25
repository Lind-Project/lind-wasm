//! Output Module for Clippy Delta Checker
//!
//! This module defines the JSON reporting format for Clippy results.
//! It serializes the run status for each manifest into `tests/ci-tests/clippy/clippy_out.json`
//! for use in CI systems, web reporting, or later analysis.

use serde::Serialize;
use std::{fs::File, path::Path};

/// A single Clippy run result for a specific Cargo manifest.
#[derive(Serialize)]
pub struct RunResult {
    /// Path to the Cargo.toml that was checked.
    pub manifest_path: String,
    /// Outcome of the Clippy run: either "success" or "failure".
    pub status: String,
    /// Full stderr output from Clippy if the run failed; omitted for successes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_body: Option<String>,
}

/// Serialize all collected Clippy results into a structured JSON file.
///
/// Saves results to `tests/ci-tests/clippy/clippy_out.json`.
/// Ensures the output folder exists before writing.
///
/// # Arguments
/// * `results` - A slice of `RunResult` entries describing each Clippy run.
///
/// # Errors
/// Returns an error if the output file cannot be created or written.
///
/// # Example
/// ```
/// output::write_results(&results)?;
/// ```
pub fn write_results(results: &[RunResult]) -> Result<(), Box<dyn std::error::Error>> {
    let output_path = Path::new("tests/ci-tests/clippy/clippy_out.json");

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let file = File::create(output_path)?;
    serde_json::to_writer_pretty(file, &serde_json::json!({ "results": results }))?;

    Ok(())
}
