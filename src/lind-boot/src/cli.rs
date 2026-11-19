use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::*;

fn parse_preloads(s: &str) -> Result<(String, PathBuf)> {
    let parts: Vec<&str> = s.splitn(2, '=').collect();
    if parts.len() != 2 {
        bail!("must contain exactly one equals character ('=')");
    }
    Ok((parts[0].into(), parts[1].into()))
}

/// Clock used by `--perf` (perf builds only).
#[cfg(feature = "lind_perf")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum PerfTimer {
    /// `clock_gettime(CLOCK_MONOTONIC_RAW)`: nanoseconds.
    Clock,
    /// `rdtsc`/`rdtscp`: CPU cycles (x86_64).
    Tsc,
}

/// Which syscalls `--perf` times (perf builds only).
#[cfg(feature = "lind_perf")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum PerfScopeArg {
    /// Only the lind-perf benchmark syscalls (2001-2999).
    Bench,
    /// Every syscall, with a per-syscall breakdown. For real programs and grates.
    All,
}

#[derive(Debug, Parser, Clone)]
#[command(name = "lind-boot")]
pub struct CliOptions {
    /// todo: Increase logging verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Enable debug mode
    #[arg(long)]
    pub debug: bool,

    /// AOT-compile a .wasm file to a .cwasm artifact and exit (no runtime needed)
    #[arg(long)]
    pub precompile: bool,

    /// Enables wasmtime backtrace details. Equivalent to wasmtime binary's
    /// WASMTIME_BACKTRACE_DETAILS=1 environment variable.
    ///
    /// Does not need any special requirements for .wasm files, for .cwasm files, this configuration must
    /// remain the same during compile and run.
    #[arg(long = "wasmtime-backtrace")]
    pub wasmtime_backtrace: bool,

    /// Enables special handling of fpcast enabled wasm binary, mainly for dynamic loading
    /// A dynamically compiled wasm binary with fpcast-emu enabled must enable this option
    #[arg(long = "enable-fpcast")]
    pub enable_fpcast: bool,

    /// Optional in-memory Wasm module bytes.
    ///
    /// This is not parsed from the command line. When present, lind-boot loads
    /// the main module from these bytes instead of reading `WASM_FILE` from disk.
    /// `WASM_FILE` is still used as guest argv[0].
    #[arg(skip)]
    pub wasm_bytes: Option<Vec<u8>>,

    /// First item is WASM file (argv[0]), rest are program args (argv[1..])
    ///
    /// Example:
    ///   lind-wasm prog.wasm a b c
    #[arg(value_name = "WASM_FILE", required = true, num_args = 1.., trailing_var_arg = true)]
    pub args: Vec<String>,

    /// Pass an environment variable to the program.
    ///
    /// The `--env FOO=BAR` form will set the environment variable named `FOO`
    /// to the value `BAR` for the guest program using WASI. The `--env FOO`
    /// form will set the environment variable named `FOO` to the same value it
    /// has in the calling process for the guest, or in other words it will
    /// cause the environment variable `FOO` to be inherited.
    #[arg(long = "env", number_of_values = 1, value_name = "NAME[=VAL]", value_parser = parse_env_var)]
    pub vars: Vec<(String, Option<String>)>,

    /// Load the given WebAssembly module before the main module
    #[arg(
        long = "preload",
        number_of_values = 1,
        value_name = "NAME=MODULE_PATH",
        value_parser = parse_preloads,
    )]
    pub preloads: Vec<(String, PathBuf)>,

    /// Host thread stack size in bytes for spawned cage/thread processes.
    /// Defaults to 64 MiB.
    #[arg(long = "thread-stack-size", default_value_t = 64 * 1024 * 1024)]
    pub thread_stack_size: usize,

    /// Internal memory reservation for Lind's own use (e.g., GC Heap) in bytes.
    #[arg(long = "lind-internal-memory-reservation", default_value_t = 10 * 1024 * 1024)]
    pub lind_internal_memory_reservation: u64,

    /// Per-layer syscall timing (perf builds only). Runs the program once per counter in a
    /// fresh lind-boot process and prints a report. `--perf` = `--perf=clock` (ns);
    /// `--perf=tsc` for CPU cycles.
    #[cfg(feature = "lind_perf")]
    #[arg(
        long,
        value_enum,
        value_name = "clock|tsc",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "clock"
    )]
    pub perf: Option<PerfTimer>,

    /// With `--perf`: only run counters whose names start with one of these prefixes,
    /// comma-separated (e.g. `threei::,lind_3i::`).
    #[cfg(feature = "lind_perf")]
    #[arg(long = "perf-filter", value_name = "PREFIX[,PREFIX...]")]
    pub perf_filter: Option<String>,

    /// With `--perf`: `bench` times only the benchmark syscalls (default); `all` times every
    /// syscall and adds a per-syscall table.
    #[cfg(feature = "lind_perf")]
    #[arg(long = "perf-scope", value_enum, default_value = "bench")]
    pub perf_scope: PerfScopeArg,

    /// With `--perf-scope=all`: number of rows in the per-syscall table.
    #[cfg(feature = "lind_perf")]
    #[arg(long = "perf-top", default_value_t = 25)]
    pub perf_top: usize,
}

pub fn parse_env_var(s: &str) -> Result<(String, Option<String>), String> {
    let mut parts = s.splitn(2, '=');
    Ok((
        parts.next().unwrap().to_string(),
        parts.next().map(|s| s.to_string()),
    ))
}

impl CliOptions {
    pub fn wasm_file(&self) -> &str {
        &self.args[0]
    }
}
