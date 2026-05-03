use crate::cli::CliOptions;
use std::sync::{Arc, Mutex, OnceLock};
use sysdefs::constants::lind_platform_const;
use wasmtime::Table;
use wasmtime_lind_3i::*;
use wasmtime_lind_common::LindEnviron;
use wasmtime_lind_multi_process::{LindCtx, LindHost};
use wasmtime_lind_utils::LindGOT;

/// The HostCtx host structure stores all relevant execution context objects:
/// `lind_environ`: argv/environ data served by the 4 host functions in lind-common;
/// `lind_fork_ctx`: the multi-process management structure, encapsulating fork/exec state;
#[derive(Default, Clone)]
pub struct HostCtx {
    pub lind_environ: Option<LindEnviron>,
    pub lind_fork_ctx: Option<LindCtx<HostCtx, CliOptions>>,
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
        }
    }

    pub fn fork_thread(&self) -> Self {
        let forked_lind_fork_ctx = self.lind_fork_ctx.as_ref().map(|ctx| ctx.fork_thread());

        Self {
            lind_environ: self.lind_environ.clone(),
            lind_fork_ctx: forked_lind_fork_ctx,
        }
    }
}

impl LindHost<HostCtx, CliOptions> for HostCtx {
    fn get_ctx(&self) -> LindCtx<HostCtx, CliOptions> {
        self.lind_fork_ctx.clone().unwrap()
    }

    fn get_ctx_mut(&mut self) -> &mut LindCtx<HostCtx, CliOptions> {
        self.lind_fork_ctx.as_mut().unwrap()
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

/// Global grate-handler registry for lind-wasm boot/runtime coordination.
///
/// This section manages the process-wide table of registered `GrateHandler`s.
/// Each grate may register exactly one handler during initialization and later
/// grate calls resolve that handler through this global table before submitting
/// work into the handler’s worker pool.
///
/// Conceptually, this table is the global entry point from cage/grate identity
/// to the runtime object that actually executes grate calls. The handler itself
/// owns the worker pool and concurrency policy, while this registry is
/// responsible for:
///
/// 1. allocating the global per-cage handler table,
/// 2. registering a handler for a newly initialized grate,
/// 3. retrieving the handler during grate-call dispatch, and
/// 4. unregistering / shutting down the handler during cleanup.
///
/// This logic lives in `lind-boot`, rather than in `lind-3i` or Wasmtime,
/// because the global table stores concrete `GrateHandler<HostCtx>` values.
/// The worker-local `Store<T>` type is parameterized by the host state `T`,
/// and in this build that `T` is the boot/runtime-specific `HostCtx` defined
/// in `lind-boot`.
///
/// As a result, the global registry must be instantiated at a layer where the
/// concrete host context type is known. If this table were moved into a lower
/// layer such as `lind-3i`, that layer would only see the generic `T` carried
/// by `Store<T>` and would not be able to materialize a global table of
/// handlers with a concrete type at compile time. In practice, that would make
/// the handler registry ill-typed unless the entire lower layer were also
/// parameterized around the concrete host context.
static GRATE_POOL: OnceLock<Vec<Mutex<Option<Arc<GrateHandler<HostCtx>>>>>> = OnceLock::new();

/// Initialize the global `GrateHandler` registry.
///
/// This function must be called exactly once during lind-wasm startup before
/// any handler is registered or retrieved. It eagerly allocates one table slot
/// per possible `cage_id`, with each slot initially empty.
///
/// The registry is indexed by cage / grate id and later stores the
/// `Arc<GrateHandler<HostCtx>>` associated with that id.
pub fn init_grate_pool() {
    GRATE_POOL.get_or_init(|| {
        (0..lind_platform_const::MAX_CAGEID)
            .map(|_| Mutex::new(None))
            .collect()
    });
}

/// Create and register the grate handler for one cage.
///
/// This function constructs a new `GrateHandler` for the given `cageid` and
/// installs it into the global registry. Registration fails if the registry
/// has not been initialized, if the cage id is out of range, or if a handler
/// has already been registered for that cage.
///
/// After successful registration, future grate calls targeting this cage can
/// retrieve the handler through the global table and submit requests into its
/// worker pool.
pub fn register_grate_handler_for_cage(
    template: &GrateTemplate<HostCtx>,
    host: HostCtx,
    cageid: u64,
) -> anyhow::Result<()> {
    // Create worker pool and handler for this cage's grate
    let handler = create_handler_for_cage(template, host, cageid, ConcurrencyMode::Parallel)?;

    // Register the handler in the global pool
    let pool = GRATE_POOL
        .get()
        .ok_or_else(|| anyhow::anyhow!("GRATE_POOL is not initialized"))?;

    let slot = pool
        .get(cageid as usize)
        .ok_or_else(|| anyhow::anyhow!("invalid cageid {}", cageid))?;

    let mut guard = slot.lock().unwrap();

    if guard.is_some() {
        anyhow::bail!("GrateHandler for cageid {} already exists", cageid);
    }

    *guard = Some(Arc::new(handler));
    Ok(())
}

/// Look up the registered grate handler for a given grate id.
///
/// This is the internal retrieval path used by grate-call dispatch. The
/// function resolves the target slot in the global registry, verifies that a
/// handler is present, and returns a cloned `Arc` to the caller.
///
/// Returning an `Arc` allows the submission path to hold a stable reference to
/// the handler even if another thread later begins cleanup or unregisters the
/// slot from the global table.
fn get_grate_handler(grate_id: u64) -> anyhow::Result<Arc<GrateHandler<HostCtx>>> {
    #[cfg(feature = "debug-grate-calls")]
    println!(
        "[lind-boot] Retrieving grate handler for grate_id {}",
        grate_id
    );
    let pool = GRATE_POOL
        .get()
        .ok_or_else(|| anyhow::anyhow!("GRATE_POOL is not initialized"))?;

    let slot = pool
        .get(grate_id as usize)
        .ok_or_else(|| anyhow::anyhow!("invalid grate_id {}", grate_id))?;

    let guard = slot.lock().unwrap();

    #[cfg(feature = "debug-grate-calls")]
    println!(
        "[lind-boot] Grate handler for grate_id {} is {}",
        grate_id,
        if guard.is_some() { "present" } else { "absent" }
    );

    guard
        .as_ref()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("grate handler {} not found", grate_id))
}

/// Submit one grate request to the handler registered for `grate_id` and
/// run it to completion, returning the result code.
///
/// This is the global dispatch entry point used by the grate-call path.
/// It first resolves the target handler from the global registry and then
/// forwards the request into that handler’s submission logic, where worker
/// acquisition, concurrency policy, and shutdown checks are enforced.
///
/// In other words, this function bridges the global grate-id lookup layer and
/// the per-handler execution layer.
///
/// This function is used on the trampoline path for all grate calls
pub fn submit_grate_request(grate_id: u64, req: GrateRequest) -> anyhow::Result<i32> {
    #[cfg(feature = "debug-grate-calls")]
    println!(
        "[lind-boot] Submitting grate request to cage {}, handler_addr: {:#x}",
        req.cageid, req.handler_addr
    );

    // Look up the handler for this grate id
    let handler = match get_grate_handler(grate_id) {
        Ok(handler) => {
            #[cfg(feature = "debug-grate-calls")]
            println!("[lind-boot] got handler");
            handler
        }
        Err(e) => {
            panic!("[lind-boot] get_grate_handler failed: {:?}", e);
        }
    };

    // Submit the request into the handler's worker pool and return the result
    handler.submit(req)
}

/// Remove the registered grate handler for `grate_id` from the global table.
///
/// This function detaches the handler from the global registry and returns the
/// owned `Arc` to the caller. After unregistration, new lookups for the same
/// grate id will fail, but any thread already holding a cloned `Arc` may still
/// continue interacting with that handler until shutdown logic completes.
///
/// todo: not integrated with actual grate teardown yet
pub fn unregister_grate_handler(grate_id: u64) -> anyhow::Result<Arc<GrateHandler<HostCtx>>> {
    let pool = GRATE_POOL
        .get()
        .ok_or_else(|| anyhow::anyhow!("GRATE_POOL is not initialized"))?;

    let slot = pool
        .get(grate_id as usize)
        .ok_or_else(|| anyhow::anyhow!("invalid grate_id {}", grate_id))?;

    let mut guard = slot.lock().unwrap();

    guard
        .take()
        .ok_or_else(|| anyhow::anyhow!("grate handler {} not found", grate_id))
}

/// Gracefully shut down and clean up the handler registered for `grate_id`.
///
/// Cleanup proceeds in two phases:
///
/// 1. unregister the handler from the global registry so no new global lookups
///    can find it,
/// 2. begin handler shutdown and wait until all in-flight grate calls have
///    completed.
///
/// This ensures that no new work is admitted while allowing already-running
/// grate calls to drain before cleanup finishes.
///
/// todo: not integrated with actual grate teardown yet
pub fn cleanup_grate_handler(grate_id: u64) -> anyhow::Result<()> {
    let handler = unregister_grate_handler(grate_id)?;

    // 1. Deactivate the handler to prevent new requests from being accepted
    // and interrupt other running stores
    handler.begin_shutdown();

    // 2. Wait for the handler to finish processing any in-flight requests and become idle.
    handler.wait_for_idle();

    Ok(())
}
