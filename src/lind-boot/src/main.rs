mod cli;
mod lind_wasmtime;

use crate::{cli::CliOptions, lind_wasmtime::execute_wasmtime};
use clap::Parser;
use rawposix::sys_calls::{rawposix_shutdown, rawposix_start};

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
    // Initialize RawPOSIX, also registered RawPOSIX syscalls to 3i
    rawposix_start(0);

    // Execute with user-selected runtime. Can be switched to other runtime implementation
    // in the future (e.g.: MPK).
    execute_wasmtime(lindboot_cli)?;

    // after all cage exits, finalize the lind
    rawposix_shutdown();

    Ok(())
}
