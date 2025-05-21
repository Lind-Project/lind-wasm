//! Output Module for Clippy Delta Checker
//!
//! This module defines the JSON and HTML reporting formats for Clippy results.
//! It serializes the run status for each manifest into `clippy_out.json` and `clippy_out.html`
//! for use in CI systems, web reporting, or later analysis.

use serde::Serialize;
use std::{fs::File, path::Path, io::Write};

/// A single Clippy run result for a specific Cargo manifest.
#[derive(Serialize)]
pub struct RunResult {
    /// Path to the Cargo.toml that was checked.
    pub manifest_path: String,
    /// Outcome of the Clippy run: either "success", "failure", or "error".
    pub status: String,
    /// Full stderr output from Clippy if the run failed; omitted for successes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_body: Option<String>,
}

/// Serialize all collected Clippy results into JSON and HTML files.
///
/// Saves results to `output_path` (JSON) and a sibling `.html` file for human viewing.
///
/// # Arguments
/// * `results` - A slice of `RunResult` entries describing each Clippy run.
/// * `output_path` - File path to the JSON output file.
///
/// # Errors
/// Returns an error if either file cannot be created or written.
pub fn write_results(results: &[RunResult], output_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let output_path = Path::new(output_path);

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Write JSON
    let json_file = File::create(output_path)?;
    serde_json::to_writer_pretty(json_file, &serde_json::json!({ "results": results }))?;

    // Write HTML next to JSON
    let html_path = output_path.with_extension("html");
    write_results_html(results, &html_path)?;

    Ok(())
}

/// Generates a human-readable HTML report.
fn write_results_html(results: &[RunResult], html_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::create(html_path)?;

    writeln!(file, "<!DOCTYPE html>")?;
    writeln!(file, "<html><head><meta charset='UTF-8'><title>Clippy Report</title>")?;
    writeln!(file, "<style>
        body {{ font-family: sans-serif; padding: 1em; }}
        .success {{ color: green; }}
        .failure, .error {{ color: red; white-space: pre-wrap; }}
        table {{ border-collapse: collapse; width: 100%; }}
        th, td {{ border: 1px solid #ccc; padding: 0.5em; text-align: left; }}
        </style></head><body>")?;

    writeln!(file, "<h1>Clippy Manifest Report</h1>")?;
    writeln!(file, "<table>")?;
    writeln!(file, "<tr><th>Manifest</th><th>Status</th><th>Errors</th></tr>")?;

    for result in results {
        let status_class = match result.status.as_str() {
            "success" => "success",
            "failure" | "error" => "failure",
            _ => "unknown",
        };

        let error_html = match &result.error_body {
            Some(body) => format!("<div class=\"{}\">{}</div>", status_class, html_escape::encode_text(body)),
            None => "".to_string(),
        };

        writeln!(
            file,
            "<tr><td>{}</td><td class=\"{}\">{}</td><td>{}</td></tr>",
            result.manifest_path,
            status_class,
            result.status,
            error_html
        )?;
    }

    writeln!(file, "</table></body></html>")?;
    Ok(())
}
