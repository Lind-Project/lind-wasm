mod cli;
mod lind_mpk;
mod lind_wasmtime;
mod shims;

use crate::{
    cli::CliOptions,
    lind_mpk::execute_mpk,
    lind_mpk::init_mpk,
    lind_wasmtime::{exec_wasm, init_wasmtime, precompile_module},
};
use typemap::{BinaryFileType, detect_binary_type};

use clap::Parser;
use libc;
use std::ffi::CString;
use std::path::Path;

use rawposix::init::{rawposix_shutdown, rawposix_start};
use sysdefs::constants::LINDFS_ROOT;
use wasmtime_lind_multi_process::CAGE_START_ID;
use wasmtime_lind_utils::LindCageManager;
use std::sync::Arc;

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

    // Initialize RawPOSIX and register RawPOSIX syscalls with 3i
    rawposix_start(0);

    // Create the shared cage lifecycle manager once, before routing to any backend.
    // Both the Wasmtime and MPK paths share the same manager so cage counts and
    // shutdown synchronisation are consistent regardless of the binary type.
    let lind_manager = Arc::new(LindCageManager::new(0));
    lind_manager.increment(); // account for the first cage about to be created

    // Initialize the Wasmtime runtime (one-time setup)
    init_wasmtime(lind_manager.clone());
    init_mpk(lind_manager.clone());

    // Register the unified clone/exec/exit shims with 3i.  This must happen
    // after rawposix_start() and before any cage is created.
    shims::register_syscall_entries();

    // Detect the binary format from the file magic and route to the
    // appropriate runtime backend:
    //   - ELF  (.so / native)  → MPK-isolated dlmopen execution
    //   - Wasm (.wasm/.cwasm)  → Wasmtime execution
    //
    // All exit semantics are normalized to a single i32 exit code inside
    // each backend. EX_SOFTWARE (70) signals a runtime-level failure.
    let binary_path = std::path::Path::new(lindboot_cli.wasm_file());
    let file_type = detect_binary_type(binary_path);

    let cage_id = CAGE_START_ID as u64;
    let result = match file_type {
        BinaryFileType::ElfSo => execute_mpk(lindboot_cli, cage_id),
        BinaryFileType::Wasm | BinaryFileType::CWasm | BinaryFileType::Unknown => {
            exec_wasm(lindboot_cli, lind_manager, cage_id)
        },
        BinaryFileType::ElfExe => {
            eprintln!("Error: Executable ELF binaries are not yet supported. Please use a shared object (.so) or Wasm binary (.wasm/.cwasm).");
            std::process::exit(sysdefs::constants::EX_SOFTWARE);
        }

    };
    match result {
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
