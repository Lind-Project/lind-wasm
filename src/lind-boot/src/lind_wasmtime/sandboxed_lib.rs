//! Sandboxed-library (library-embedding) entry points.
//!
//! Unlike `execute_wasmtime`, which instantiates a module, runs its entry point,
//! and tears the runtime down, this module brings up the Lind runtime once and
//! keeps a module instantiated as a long-lived "reactor" so its exported functions
//! can be called on demand from host (`extern "C"`) code. This is the foundation
//! for exposing a wasm-sandboxed library as a native shared object.
//!
//! It shares the entire instantiation path (static *and* dynamic/dylink) with the
//! binary via `prepare_main_instance`; it simply keeps the resulting instance alive
//! instead of running an entry point.
//!
//! Current scope (PoC): scalar (`i32`) arguments and results (no marshalling), a
//! single global instance, no chroot (a library must not chroot its host), and calls
//! are serialized by the caller (e.g. a `Mutex`).

use anyhow::{Context, Result, anyhow};
use std::sync::{Arc, Mutex};

use wasmtime::{Engine, Instance, Linker, Module, Store, Val};
use wasmtime_lind_multi_process::CAGE_START_ID;
use wasmtime_lind_utils::LindCageManager;

use super::execute::{
    ensure_global_runtime_init, make_wasmtime_config, prepare_main_instance,
    read_main_wasm_or_cwasm,
};
use super::host::HostCtx;
use crate::cli::CliOptions;

/// A sandboxed wasm library: the live `Store` + `Instance`, kept alive together with
/// the engine/linker/module/cage-manager they depend on, so exports can be called
/// repeatedly without re-instantiating or re-initializing the runtime.
pub struct SandboxedLib {
    store: Store<HostCtx>,
    instance: Instance,
    // Kept alive for the lifetime of the sandboxed-lib instance.
    _cageid: u64,
    _linker: Arc<Mutex<Linker<HostCtx>>>,
    _engine: Engine,
    _module: Module,
    _lind_manager: Arc<LindCageManager>,
}

/// Bring up the Lind runtime (once) and instantiate the module named by `cli` (its
/// `args[0]`, or `cli.wasm_bytes` if set) as a long-lived sandboxed library,
/// returning a handle whose exports can be called via [`SandboxedLib::call_scalar`].
///
/// Works for both static and dynamic (dylink) modules — it reuses the binary's
/// `prepare_main_instance`, so the dynamic-linking setup is identical.
pub fn init_sandboxed_lib(cli: CliOptions) -> Result<SandboxedLib> {
    // One-time, process-global runtime init (RawPOSIX + 3i + vmctx pool). Idempotent.
    ensure_global_runtime_init();

    let lind_manager = Arc::new(LindCageManager::new(0));
    lind_manager.increment();

    let cageid = CAGE_START_ID as u64;

    let config = make_wasmtime_config(cli.wasmtime_backtrace, cli.enable_fpcast);
    let engine = Engine::new(&config)
        .map_err(anyhow::Error::from)
        .context("failed to create execution engine")?;

    let module = read_main_wasm_or_cwasm(&engine, &cli)?;

    // Instantiate + per-cage setup, but do not run an entry point. Wrapped in the
    // ambient tokio runtime, as `prepare_main_instance` requires.
    let (store, linker, instance) = wasmtime_wasi::runtime::with_ambient_tokio_runtime(|| {
        prepare_main_instance(cli, lind_manager.clone(), &engine, &module, cageid)
    })?;

    Ok(SandboxedLib {
        store,
        instance,
        _cageid: cageid,
        _linker: linker,
        _engine: engine,
        _module: module,
        _lind_manager: lind_manager,
    })
}

impl SandboxedLib {
    /// Call an exported function that takes `i32` arguments and returns a single
    /// `i32`. (PoC scope — no pointer/buffer/struct marshalling.)
    pub fn call_scalar(&mut self, name: &str, args: &[i32]) -> Result<i32> {
        let func = self
            .instance
            .get_func(&mut self.store, name)
            .ok_or_else(|| anyhow!("exported function `{}` not found", name))?;

        let params: Vec<Val> = args.iter().map(|a| Val::I32(*a)).collect();
        let n_results = func.ty(&self.store).results().len();
        let mut results = vec![Val::null_func_ref(); n_results];

        // Executing guest code runs under an ambient tokio runtime, matching the
        // binary's invocation path.
        wasmtime_wasi::runtime::with_ambient_tokio_runtime(|| {
            func.call(&mut self.store, &params, &mut results)
        })
        .map_err(anyhow::Error::from)
        .with_context(|| format!("failed to call `{}`", name))?;

        match results.first() {
            Some(Val::I32(v)) => Ok(*v),
            other => Err(anyhow!(
                "expected an i32 result from `{}`, got {:?}",
                name,
                other
            )),
        }
    }
}
