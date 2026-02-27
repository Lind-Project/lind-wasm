mod cli;
mod lind_wasmtime;
mod perf;

use crate::{
    cli::CliOptions,
    lind_wasmtime::{execute_wasmtime, precompile_module},
};
use clap::Parser;
use rawposix::init::{rawposix_shutdown, rawposix_start};

/// Entry point of the lind-boot executable.
///
/// The expected invocation follows: the first non-flag argument specifies the
/// Wasm binary to execute and all remaining arguments are forwarded verbatim to
/// the guest program:
///
///     lind-boot [flags...] wasm_file.wasm arg1 arg2 ...
///
/// All process lifecycle management, runtime initialization, and error
/// handling semantics are delegated to `execute.rs`.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let lindboot_cli = CliOptions::parse();

    // AOT-compile only — no runtime needed
    if lindboot_cli.precompile {
        precompile_module(&lindboot_cli)?;
        return Ok(());
    }

    // Perf mode is a "one counter per run" workflow:
    // initialize counters once, then rerun the same workload with each counter
    // exclusively enabled so measurements do not overlap.
    if let Some(kind) = lindboot_cli.perf_timer_kind() {
        perf::perf_init(kind);

        let counters = perf::all_counter_names();

        for counter in counters {
            perf::enable_one_counter(counter);

            // Each perf sample gets a fresh RawPOSIX lifecycle boundary.
            rawposix_start(0);
            let _ = execute_wasmtime(lindboot_cli.clone());
            rawposix_shutdown();
        }

        perf::perf_report();

        return Ok(());
    }

    // Initialize RawPOSIX and register RawPOSIX syscalls with 3i
    rawposix_start(0);

    // Execute with user-selected runtime. Can be switched to other runtime implementation
    // in the future (e.g.: MPK).
    execute_wasmtime(lindboot_cli)?;

    // after all cage exits, finalize the lind
    rawposix_shutdown();

    Ok(())
}
