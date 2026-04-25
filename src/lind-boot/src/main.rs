mod cli;
mod lind_wasmtime;
mod perf;

use crate::{
    cli::CliOptions,
    lind_wasmtime::{execute_wasmtime, precompile_module},
};

use clap::Parser;
use libc;
use std::ffi::CString;
use std::path::Path;

use rawposix::init::{rawposix_shutdown, rawposix_start};
use sysdefs::constants::LINDFS_ROOT;

/// Helper function which `chroot`s to `lindfs`.
///
/// - check if LINDFS_ROOT exists
/// - chroot to LINDFS_ROOT
/// - chdir to new '/'
fn chroot_to_lindfs() {
    unsafe {
        let lindfs_path = CString::new(LINDFS_ROOT).unwrap();

        if !Path::new(LINDFS_ROOT).is_dir() {
            panic!("The configured lindfs does not exist: {}", LINDFS_ROOT);
        }

        let ret = libc::chroot(lindfs_path.as_ptr());
        if ret != 0 {
            panic!(
                "Failed to chroot to {}: {}",
                LINDFS_ROOT,
                std::io::Error::last_os_error()
            );
        }
        let root = CString::new("/").unwrap();
        let ret = libc::chdir(root.as_ptr());
        if ret != 0 {
            panic!(
                "Failed to chdir to / after chroot: {}",
                std::io::Error::last_os_error()
            )
        }
    }
}

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
    // Not a precompile command, chroot to lindfs
    chroot_to_lindfs();

    // Check if --perf is enabled and avaible to decide whether to run in benchmarking mode.
    if let Some(kind) = lindboot_cli.perf_timer_kind() {
        // Initialize all counters.
        perf::perf_init(kind);

        let counters = perf::all_counter_names();

        // Iterate over all counters:
        // - Exclusively enable the counter
        // - Run the program to gather timing data.
        for counter in counters {
            perf::enable_one_counter(counter);

            // Each sample run gets a fresh RawPOSIX lifecycle boundary to imitate actual
            // behaviour.
            rawposix_start(0);
            let _ = execute_wasmtime(lindboot_cli.clone());
            rawposix_shutdown();
        }

        // Output final numbers to stdout.
        perf::perf_report();

        return Ok(());
    }

    // Initialize RawPOSIX and register RawPOSIX syscalls with 3i
    rawposix_start(0);

    // Execute the selected runtime backend and translate its unified
    // execution result into a host-level process exit status.
    //
    // At this layer, we intentionally do NOT interpret Wasm return
    // conventions or runtime-specific details. All exit semantics
    // (e.g., proc_exit, return values, traps) are already normalized
    // inside `execute_wasmtime` into a single `i32` exit code.
    //
    // This design keeps the runtime backend pluggable (e.g., Wasmtime,
    // MPK-based runtime, SGX-enclosed runtime) while preserving a
    // consistent host process contract: exit(code) on success,
    // If the runtime backend fails before producing a normalized
    // program exit code, terminate with EX_SOFTWARE (70) to signal
    // a runtime-level failure rather than a cage-provided exit code.
    match execute_wasmtime(lindboot_cli) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("{:?}", e);
            std::process::exit(sysdefs::constants::EX_SOFTWARE);
        }
    }

    // after all cage exits, finalize the lind
    rawposix_shutdown();

    Ok(())
}
