use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::*;

/// How a `--preload`ed library's un-interposed exports are linked.
///
/// `Mixed` (default) is today's behavior: a symbol with a registered grate
/// handler is dispatched through the grate; anything else links straight to
/// the library's own, real implementation.
///
/// `Interposed` gives a hard, address-space-isolation guarantee for that
/// library: a symbol either dispatches through a grate, or -- if nothing
/// registered a handler for it -- the call traps instead of ever running the
/// library's real implementation locally. No per-symbol exceptions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreloadMode {
    Mixed,
    Interposed,
}

fn parse_preloads(s: &str) -> Result<(String, PathBuf, PreloadMode)> {
    let (rest, mode) = if let Some(r) = s.strip_suffix(":interposed") {
        (r, PreloadMode::Interposed)
    } else if let Some(r) = s.strip_suffix(":mixed") {
        (r, PreloadMode::Mixed)
    } else {
        (s, PreloadMode::Mixed)
    };
    let parts: Vec<&str> = rest.splitn(2, '=').collect();
    if parts.len() != 2 {
        bail!("must contain exactly one equals character ('=')");
    }
    Ok((parts[0].into(), parts[1].into(), mode))
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

    /// Load the given WebAssembly module before the main module.
    ///
    /// Optionally suffix with `:interposed` to require every one of this
    /// library's exports to either dispatch through a registered grate
    /// handler or trap -- e.g. `--preload env=/lib/libm.cwasm:interposed`.
    /// Defaults to `:mixed` (today's behavior: dispatch what's registered,
    /// fall through to the real implementation otherwise).
    #[arg(
        long = "preload",
        number_of_values = 1,
        value_name = "NAME=MODULE_PATH[:mixed|:interposed]",
        value_parser = parse_preloads,
    )]
    pub preloads: Vec<(String, PathBuf, PreloadMode)>,

    /// Host thread stack size in bytes for spawned cage/thread processes.
    /// Defaults to 64 MiB.
    #[arg(long = "thread-stack-size", default_value_t = 64 * 1024 * 1024)]
    pub thread_stack_size: usize,
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
