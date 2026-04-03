use crate::cli::CliOptions;
use std::sync::{Arc, Mutex};
use wasmtime::{TypedFunc, Table};
use wasmtime_lind_common::LindEnviron;
use wasmtime_lind_multi_process::{LindCtx, LindHost};
use wasmtime_lind_utils::LindGOT;

/// Function type for the `pass_fptr_to_wt` function used as an entry point for grate-run syscalls.
pub type PassFptrTyped = TypedFunc<
    (
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
    ),
    i32,
>;

/// The HostCtx host structure stores all relevant execution context objects:
/// `lind_environ`: argv/environ data served by the 4 host functions in lind-common;
/// `lind_fork_ctx`: the multi-process management structure, encapsulating fork/exec state;
/// `wasi_threads`: which manages WASI thread-related capabilities.
/// `pass_fptr_func`: the dispatcher function defined in the grate's WASM module. Cached after the
/// first invocation.
#[derive(Default, Clone)]
pub struct HostCtx {
    pub lind_environ: Option<LindEnviron>,
    pub lind_fork_ctx: Option<LindCtx<HostCtx, CliOptions>>,
    pub pass_fptr_func: Option<PassFptrTyped>,
}

impl HostCtx {
    /// Performs a partial deep clone of the host context. It explicitly forks the
    /// lind_environ (argv/env) and the lind multi-process context (`lind_fork_ctx`).
    /// Other parts of the context, such as `wasi_threads`, are shared between forks
    /// since they are not required to be process-isolated.
    pub fn fork(&self) -> Self {
        let forked_lind_environ = self.lind_environ.as_ref().map(|e| e.fork());

        let forked_lind_fork_ctx = self.lind_fork_ctx.as_ref().map(|ctx| ctx.fork_process());

        Self {
            lind_environ: forked_lind_environ,
            lind_fork_ctx: forked_lind_fork_ctx,
            pass_fptr_func: None,
        }
    }
}

impl LindHost<HostCtx, CliOptions> for HostCtx {
    fn get_ctx(&self) -> LindCtx<HostCtx, CliOptions> {
        self.lind_fork_ctx.clone().unwrap()
    }
}

pub struct DylinkMetadata {
    pub dylink_enabled: bool,
    pub got: Option<Arc<Mutex<LindGOT>>>,
    pub table: Option<Table>,
    pub epoch_handler: Option<u64>,
}

impl DylinkMetadata {
    pub fn new(dylink_enabled: bool) -> Self {
        DylinkMetadata {
            dylink_enabled,
            got: None,
            table: None,
            epoch_handler: None,
        }
    }
}
