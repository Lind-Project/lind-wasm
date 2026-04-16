use crate::cli::CliOptions;
use std::sync::{Arc, Mutex, OnceLock};
use wasmtime::{Table, TypedFunc};
use wasmtime_lind_common::LindEnviron;
use wasmtime_lind_multi_process::{LindCtx, LindHost};
use wasmtime_lind_utils::LindGOT;
use wasmtime_lind_3i::*;
use sysdefs::constants::lind_platform_const;

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


static GRATE_POOL: OnceLock<Vec<Mutex<Option<Arc<GrateHandler<HostCtx>>>>>> = OnceLock::new();

/// Initialize the global `GrateHandler` pool.
///
/// This function must be called exactly once during lind-wasm startup, before any `GrateHandler` is
/// pushed to or retrieved from the pool. It eagerly allocates one empty queue per possible `cage_id`.
pub fn init_grate_pool() {
    GRATE_POOL.get_or_init(|| {
        (0..lind_platform_const::MAX_CAGEID)
            .map(|_| Mutex::new(None))
            .collect()
    });
}

pub fn register_grate_handler_for_cage(
    template: &GrateTemplate<HostCtx>,
    host: HostCtx,
    cageid: u64,
) -> anyhow::Result<()>
{
    let handler = create_handler_for_cage(
        template,
        host,
        cageid,
        ConcurrencyMode::Serialized,
    )?;

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

fn get_grate_handler(grate_id: u64) -> anyhow::Result<Arc<GrateHandler<HostCtx>>> {
    println!("[lind-boot] Retrieving grate handler for grate_id {}", grate_id);
    let pool = GRATE_POOL
        .get()
        .ok_or_else(|| anyhow::anyhow!("GRATE_POOL is not initialized"))?;

    let slot = pool
        .get(grate_id as usize)
        .ok_or_else(|| anyhow::anyhow!("invalid grate_id {}", grate_id))?;

    let guard = slot.lock().unwrap();

    println!("[lind-boot] Grate handler for grate_id {} is {}", grate_id, if guard.is_some() { "present" } else { "absent" });

    guard
        .as_ref()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("grate handler {} not found", grate_id))
}

pub fn submit_grate_request(grate_id: u64, req: GrateRequest) -> anyhow::Result<i32> {
    println!("[lind-boot] Submitting grate request to cage {}, handler_addr: {:#x}", req.cageid, req.handler_addr);
    let handler = match get_grate_handler(grate_id) {
        Ok(handler) => {
            println!("[lind-boot] got handler");
            handler
        }
        Err(e) => {
            panic!("[lind-boot] get_grate_handler failed: {:?}", e);
        }
    };
    handler.submit(req)
}

pub fn unregister_grate_handler(
    grate_id: u64,
) -> anyhow::Result<Arc<GrateHandler<HostCtx>>> {
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

pub fn cleanup_grate_handler(grate_id: u64) -> anyhow::Result<()> {
    let handler = unregister_grate_handler(grate_id)?;

    // 1. Deactivate the handler to prevent new requests from being accepted
    // and interrupt other running stores
    handler.begin_shutdown();

    // 2. Wait for the handler to finish processing any in-flight requests and become idle.
    handler.wait_for_idle();

    Ok(())
}
