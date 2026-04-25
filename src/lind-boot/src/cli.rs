use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::*;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum PerfTimer {
    /// Use `clock_gettime(CLOCK_MONOTONIC_RAW)` based timing.
    Clock,
    /// Use RDTSC/RDTSCP cycle counter timing.
    Tsc,
}

fn parse_preloads(s: &str) -> Result<(String, PathBuf)> {
    let parts: Vec<&str> = s.splitn(2, '=').collect();
    if parts.len() != 2 {
        bail!("must contain exactly one equals character ('=')");
    }
    Ok((parts[0].into(), parts[1].into()))
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

    /// Get performance information for the running module.
    ///
    /// `--perf` defaults to `clock`; pass `--perf=tsc` for cycle-based timing.
    ///
    /// `--perf` is always accepted by the CLI, but execution only proceeds when
    /// lind-boot is compiled with the crate feature `lind_perf` (which wires
    /// `lind-perf/enabled`).
    #[arg(
        long,
        value_enum,
        default_missing_value = "clock",
        value_name = "clock|tsc",
        num_args = 0..=1,
        require_equals = true,
    )]
    pub perf: Option<PerfTimer>,

    /// Load the given WebAssembly module before the main module
    #[arg(
        long = "preload",
        number_of_values = 1,
        value_name = "NAME=MODULE_PATH",
        value_parser = parse_preloads,
    )]
    pub preloads: Vec<(String, PathBuf)>,
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

    pub fn perf_timer_kind(&self) -> Option<lind_perf::TimerKind> {
        // Runtime gate for the perf CLI path:
        // - if lind-boot was compiled without `lind_perf`, reject `--perf` early
        //   with a clear error.
        // - otherwise map the CLI timer selection to lind-perf's timer backend.
        match lind_perf::ENABLED {
            false => match self.perf {
                Some(_) => {
                    eprintln!("--perf needs compilation with the feature `lind_perf` enabled.");
                    std::process::exit(1);
                }
                None => None,
            },
            true => match self.perf {
                Some(PerfTimer::Clock) => Some(lind_perf::TimerKind::Clock),
                Some(PerfTimer::Tsc) => Some(lind_perf::TimerKind::Rdtsc),
                None => None,
            },
        }
    }
}
