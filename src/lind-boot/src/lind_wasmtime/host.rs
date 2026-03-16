use crate::cli::CliOptions;
use std::sync::Arc;
use wasmtime_lind_common::LindEnviron;
use wasmtime_lind_multi_process::{LindCtx, LindHost};
use wasmtime_wasi_threads::WasiThreadsCtx;

/// The HostCtx host structure stores all relevant execution context objects:
/// `lind_environ`: argv/environ data served by the 4 host functions in lind-common;
/// `lind_fork_ctx`: the multi-process management structure, encapsulating fork/exec state;
/// `wasi_threads`: which manages WASI thread-related capabilities.
#[derive(Default, Clone)]
pub struct HostCtx {
    pub lind_environ: Option<LindEnviron>,
    pub wasi_threads: Option<Arc<WasiThreadsCtx<HostCtx>>>,
    pub lind_fork_ctx: Option<LindCtx<HostCtx, CliOptions>>,
}

impl HostCtx {
    /// Performs a partial deep clone of the host context. It explicitly forks the
    /// lind_environ (argv/env) and the lind multi-process context (`lind_fork_ctx`).
    /// Other parts of the context, such as `wasi_threads`, are shared between forks
    /// since they are not required to be process-isolated.
    pub fn fork(&self) -> Self {
        let forked_lind_environ = self.lind_environ.as_ref().map(|e| e.fork());

        let forked_lind_fork_ctx = self.lind_fork_ctx.as_ref().map(|ctx| ctx.fork());

        Self {
            lind_environ: forked_lind_environ,
            lind_fork_ctx: forked_lind_fork_ctx,
            wasi_threads: self.wasi_threads.clone(),
        }
    }
}

impl LindHost<HostCtx, CliOptions> for HostCtx {
    fn get_ctx(&self) -> LindCtx<HostCtx, CliOptions> {
        self.lind_fork_ctx.clone().unwrap()
    }
}
