mod cli;
mod grateos_wasmtime;

use crate::{
    cli::CliOptions,
    grateos_wasmtime::{execute_wasmtime, precompile_module},
};

use clap::Parser;
use libc;
use std::ffi::CString;
use std::path::Path;

use rawposix::init::{rawposix_shutdown, rawposix_start};
use sysdefs::constants::GRATEOSFS_ROOT;
use sysdefs::logging::{config_from_env, init_grateos_logger};

/// Helper function which `chroot`s to `grateosfs`.
///
/// - check if GRATEOSFS_ROOT exists
/// - chroot to GRATEOSFS_ROOT
/// - chdir to new '/'
fn chroot_to_grateosfs() {
    unsafe {
        let grateosfs_path = CString::new(GRATEOSFS_ROOT).unwrap();

        if !Path::new(GRATEOSFS_ROOT).is_dir() {
            panic!("The configured grateosfs does not exist: {}", GRATEOSFS_ROOT);
        }

        let ret = libc::chroot(grateosfs_path.as_ptr());
        if ret != 0 {
            panic!(
                "Failed to chroot to {}: {}",
                GRATEOSFS_ROOT,
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

/// Entry point of the grateos-boot executable.
///
/// The expected invocation follows: the first non-flag argument specifies the
/// Wasm binary to execute and all remaining arguments are forwarded verbatim to
/// the guest program:
///
///     grateos-boot [flags...] wasm_file.wasm arg1 arg2 ...
///
/// All process lifecycle management, runtime initialization, and error
/// handling semantics are delegated to `execute.rs`.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the GrateOS logger from environment variables before anything else.
    // Falls back to defaults (stderr, PanicAndExit, all categories) on error.
    let _ = init_grateos_logger(config_from_env().unwrap_or_default());

    let grateosboot_cli = CliOptions::parse();

    // AOT-compile only — no runtime needed
    if grateosboot_cli.precompile {
        precompile_module(&grateosboot_cli)?;
        return Ok(());
    }

    // Not a precompile command, chroot to grateosfs
    chroot_to_grateosfs();

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
    match execute_wasmtime(grateosboot_cli) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("{:?}", e);
            std::process::exit(sysdefs::constants::EX_SOFTWARE);
        }
    }

    // after all cage exits, finalize the grateos
    rawposix_shutdown();

    Ok(())
}
