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

/// Preloads for the embedding (`.so`) path, taken from the `LIND_PRELOAD` env var.
///
/// Unlike the `lind_run` binary — which is chrooted into lindfs, so it can preload
/// `env=/lib/libc.cwasm` by a lindfs-relative path — a loaded `.so` is not chrooted
/// and doesn't know the repo layout, so the caller passes **host** paths. The syntax
/// matches the CLI `--preload` flag (`name=path`), comma-separated for several:
///
/// ```text
/// LIND_PRELOAD="env=/abs/lindfs/lib/libc.cwasm,env=/abs/lindfs/lib/libm.cwasm"
/// ```
fn preloads_from_env() -> Vec<(String, PathBuf)> {
    match std::env::var("LIND_PRELOAD") {
        Ok(s) => s
            .split(',')
            .map(str::trim)
            .filter(|e| !e.is_empty())
            .map(|e| {
                parse_preloads(e).unwrap_or_else(|err| panic!("invalid LIND_PRELOAD entry `{e}`: {err}"))
            })
            .collect(),
        Err(_) => Vec::new(),
    }
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

    /// Instead of running the module's entry point (`_start`), instantiate the
    /// module as a long-lived "reactor"/library and call a single exported function
    /// by name, then print its return value(s) and exit.
    ///
    /// Integer arguments to the called function are taken from the trailing
    /// program args (everything after WASM_FILE).
    ///
    /// Example:
    ///   lind-boot --call add add_sub.cwasm 2 3
    #[arg(long = "call", value_name = "EXPORT_NAME")]
    pub call: Option<String>,

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

    /// Build a minimal `CliOptions` for embedding the runtime as a sandboxed
    /// library: load the main module from `module_path`, run no entry point, and
    /// use default settings for everything else.
    ///
    /// Provided so embedders (e.g. a cdylib wrapper) don't have to track the full
    /// field set as the CLI evolves.
    pub fn for_sandboxed_lib(module_path: impl Into<String>) -> Self {
        CliOptions {
            verbose: 0,
            debug: false,
            precompile: false,
            wasmtime_backtrace: false,
            enable_fpcast: false,
            call: None,
            wasm_bytes: None,
            args: vec![module_path.into()],
            vars: Vec::new(),
            preloads: preloads_from_env(),
            thread_stack_size: 64 * 1024 * 1024,
        }
    }
}
