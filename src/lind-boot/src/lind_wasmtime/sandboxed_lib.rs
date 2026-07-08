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

use wasmtime::{AsContextMut, Engine, Instance, Linker, Module, Store, Val};
use wasmtime_lind_multi_process::CAGE_START_ID;
use wasmtime_lind_utils::LindCageManager;

use super::execute::{
    ensure_global_runtime_init, make_wasmtime_config, prepare_main_instance,
    read_main_wasm_or_cwasm,
};
use super::host::HostCtx;
use crate::cli::CliOptions;

/// A host-side argument to a guest call.
///
/// Scalars pass straight through (a wasm `i32` *is* a native `int`). A `Buf` is the
/// first step of *marshalling*: its bytes are copied into fresh guest linear memory
/// (via the guest allocator) and the call receives the resulting **guest offset**,
/// because a host pointer is meaningless inside the guest's own address space. The
/// copy is freed after the call returns.
///
/// For a C string, pass the bytes including the trailing NUL as a `Buf`.
pub enum Arg<'a> {
    /// A scalar `i32`, passed by value.
    I32(i32),
    /// A byte buffer copied into the guest; the call receives its guest offset.
    Buf(&'a [u8]),
}

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

    /// Call an exported function, marshalling pointer arguments through guest memory.
    ///
    /// Each [`Arg::Buf`] is copied into freshly guest-`malloc`'d memory and passed as
    /// its guest offset; the copies are freed after the call (even on failure).
    /// Returns the first result widened to `i64` (covers `i32`/`usize`-shaped returns).
    ///
    /// The guest module must export `guest_malloc`/`guest_free` and its linear memory
    /// as `memory` (lind_compile exports `memory` by default).
    pub fn call(&mut self, name: &str, args: &[Arg]) -> Result<i64> {
        // Marshal in: copy each buffer arg into the guest, collecting offsets to free.
        let mut to_free: Vec<u32> = Vec::new();
        let mut params: Vec<Val> = Vec::with_capacity(args.len());
        for a in args {
            match a {
                Arg::I32(v) => params.push(Val::I32(*v)),
                Arg::Buf(bytes) => {
                    let ptr = self.copy_in(bytes)?;
                    to_free.push(ptr);
                    params.push(Val::I32(ptr as i32));
                }
            }
        }

        let func = self
            .instance
            .get_func(&mut self.store, name)
            .ok_or_else(|| anyhow!("exported function `{}` not found", name))?;
        let n_results = func.ty(&self.store).results().len();
        let mut results = vec![Val::null_func_ref(); n_results];

        let call_res = wasmtime_wasi::runtime::with_ambient_tokio_runtime(|| {
            func.call(&mut self.store, &params, &mut results)
        });

        // Free the marshalled buffers regardless of whether the call succeeded.
        for ptr in to_free {
            let _ = self.guest_free(ptr);
        }
        call_res
            .map_err(anyhow::Error::from)
            .with_context(|| format!("failed to call `{}`", name))?;

        match results.first() {
            Some(Val::I32(v)) => Ok(*v as i64),
            Some(Val::I64(v)) => Ok(*v),
            other => Err(anyhow!(
                "expected an integer result from `{}`, got {:?}",
                name,
                other
            )),
        }
    }

    /// `(host base pointer, byte length)` of the guest's linear memory.
    ///
    /// In lind's dynamic build the memory is *imported/shared*, so the main instance
    /// doesn't expose it as a named `memory` export (`get_memory(.., "memory")` returns
    /// `None`) and it isn't available as an `unshared` `wasmtime::Memory` either. We
    /// take the base + size the same way the syscall layer's `get_memory_base_and_size`
    /// does, and read/write through the pointer.
    fn guest_mem(&mut self) -> Result<(*mut u8, usize)> {
        let em = self
            .store
            .as_context_mut()
            .0
            .all_memories()
            .next()
            .ok_or_else(|| anyhow!("guest store has no linear memory"))?;
        if let Some(base) = em.shared_base_ptr() {
            // Shared memory (lind's dynamic build): read the current length off the
            // VMMemoryDefinition, mirroring get_memory_base_and_size.
            let size = unsafe {
                let vm = em.shared().expect("shared memory");
                (*vm.vmmemory_ptr().as_ptr())
                    .current_length
                    .load(std::sync::atomic::Ordering::SeqCst)
            };
            Ok((base, size))
        } else {
            let m = em.unshared().expect("memory is neither shared nor unshared");
            let base = m.data_ptr(&self.store);
            let size = m.data_size(&self.store);
            Ok((base, size))
        }
    }

    /// Allocate `n` bytes inside the guest via its exported `guest_malloc`.
    fn guest_malloc(&mut self, n: usize) -> Result<u32> {
        let f = self
            .instance
            .get_typed_func::<i32, i32>(&mut self.store, "guest_malloc")
            .map_err(anyhow::Error::from)
            .context("guest must export `guest_malloc(i32) -> i32`")?;
        let ptr = wasmtime_wasi::runtime::with_ambient_tokio_runtime(|| {
            f.call(&mut self.store, n as i32)
        })
        .map_err(anyhow::Error::from)
        .context("guest_malloc failed")?;
        Ok(ptr as u32)
    }

    /// Free a guest allocation via the guest's exported `guest_free`.
    fn guest_free(&mut self, ptr: u32) -> Result<()> {
        let f = self
            .instance
            .get_typed_func::<i32, ()>(&mut self.store, "guest_free")
            .map_err(anyhow::Error::from)
            .context("guest must export `guest_free(i32)`")?;
        wasmtime_wasi::runtime::with_ambient_tokio_runtime(|| f.call(&mut self.store, ptr as i32))
            .map_err(anyhow::Error::from)
            .context("guest_free failed")
    }

    /// Copy `bytes` into a fresh guest allocation; returns the guest offset.
    fn copy_in(&mut self, bytes: &[u8]) -> Result<u32> {
        let ptr = self.guest_malloc(bytes.len())?;
        // Fetch base+size AFTER malloc, so a memory growth is reflected.
        let (base, size) = self.guest_mem()?;
        let off = ptr as usize;
        let end = off
            .checked_add(bytes.len())
            .ok_or_else(|| anyhow!("marshalled buffer length overflow"))?;
        if end > size {
            return Err(anyhow!(
                "marshalled buffer [{off}..{end}] exceeds guest memory size {size}"
            ));
        }
        // SAFETY: `base` is the host address of the guest's linear memory (obtained
        // the same way the lind syscall layer obtains it), `[off, end)` is bounds-
        // checked against its current size, and `bytes` is a distinct host allocation.
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), base.add(off), bytes.len());
        }
        Ok(ptr)
    }
}
