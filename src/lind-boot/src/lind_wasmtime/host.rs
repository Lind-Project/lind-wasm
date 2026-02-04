use crate::cli::CliOptions;
use std::sync::Arc;
use wasi_common;
use wasmtime_lind_multi_process::{LindCtx, LindHost};
use wasmtime_wasi_threads::WasiThreadsCtx;

/// The HostCtx host structure stores all relevant execution context objects
/// `preview1_ctx`: the WASI preview1 context (used by glibc and POSIX emulation);
/// `lind_fork_ctx`: the multi-process management structure, encapsulating fork/exec state;
/// `wasi_threads`: which manages WASI thread-related capabilities.
#[derive(Default, Clone)]
pub struct HostCtx {
    pub preview1_ctx: Option<wasi_common::WasiCtx>,
    pub wasi_threads: Option<Arc<WasiThreadsCtx<HostCtx>>>,
    pub lind_fork_ctx: Option<LindCtx<HostCtx, CliOptions>>,
}

/// This implementation allows HostCtx to be used where a mutable reference to `wasi_common::WasiCtx`
/// is expected.
impl AsMut<wasi_common::WasiCtx> for HostCtx {
    fn as_mut(&mut self) -> &mut wasi_common::WasiCtx {
        self.preview1_ctx
            .as_mut()
            .expect("preview1_ctx must be initialized before use")
    }
}

impl HostCtx {
    /// Performs a partial deep clone of the host context. It explicitly forks the WASI preview1
    /// context(`preview1_ctx`), the lind multi-process context (`lind_fork_ctx`). Other parts of
    /// the context, such as `wasi_threads`, are shared between forks since they are not required
    /// to be process-isolated.
    pub fn fork(&self) -> Self {
        // we want to do a real fork for wasi_preview1 context since glibc uses the environment variable
        // related interface here
        let forked_preview1_ctx = match &self.preview1_ctx {
            Some(ctx) => Some(ctx.fork()),
            None => None,
        };

        // and we also want to fork lind-multi-process context
        let forked_lind_fork_ctx = match &self.lind_fork_ctx {
            Some(ctx) => Some(ctx.fork()),
            None => None,
        };

        let forked_host = Self {
            preview1_ctx: forked_preview1_ctx,
            lind_fork_ctx: forked_lind_fork_ctx,
            wasi_threads: self.wasi_threads.clone(),
        };

        return forked_host;
    }
}

impl LindHost<HostCtx, CliOptions> for HostCtx {
    fn get_ctx(&self) -> LindCtx<HostCtx, CliOptions> {
        self.lind_fork_ctx.clone().unwrap()
    }
}
