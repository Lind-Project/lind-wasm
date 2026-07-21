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
/// Scalars (`I32`/`USize`) pass straight through. A wasm `i32` *is* a native `int`,
/// and wasm32 `size_t` is a 4-byte value. Everything else is *marshalled*: a host
/// pointer is meaningless inside the guest's own linear-memory address space, so
/// buffers are copied into guest memory (via the guest allocator) and the call
/// receives the resulting **guest offset**. Allocations are freed after the call.
///
/// - `Buf` is an input buffer copied *into* the guest (e.g. a C string, incl. its NUL).
/// - `Out` is a caller-allocated output buffer: the guest writes into it and the bytes
///   are copied back out into `dst`. `dst.len()` is the buffer's capacity.
/// - `OutLen` is a `size_t *` output parameter: the guest writes a length there, which
///   is read back into `*dst` (and can drive an `Out` buffer's copy-back length).
pub enum Arg<'a> {
    /// A scalar `i32`, passed by value.
    I32(i32),
    /// A scalar `size_t`, passed by value (wasm32 `size_t` is 4 bytes).
    USize(usize),
    /// An input buffer copied into the guest; the call receives its guest offset.
    Buf(&'a [u8]),
    /// A caller-allocated output buffer. The guest writes into it; after the call the
    /// first `len`-determined bytes are copied back into `dst`. `dst.len()` = capacity.
    Out { dst: &'a mut [u8], len: OutLen },
    /// A `size_t *` output parameter. The guest writes a length; it is read back into
    /// `*dst` and is available to resolve an [`OutLen::FromArg`] on an `Out` buffer.
    OutLen(&'a mut usize),
}

/// How many bytes to copy back out of an [`Arg::Out`] buffer. A per-function contract
/// that the C type alone can't express, so it is declared per call site.
#[derive(Clone, Copy)]
pub enum OutLen {
    /// Bytes written = the function's return value (clamped to the capacity).
    Ret,
    /// The buffer is a C string: copy up to and including the first NUL.
    Nul,
    /// The whole buffer was filled: copy the full capacity.
    Cap,
    /// Bytes written = the value the guest wrote into the [`Arg::OutLen`] at this index
    /// in the args slice (clamped to the capacity).
    FromArg(usize),
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
    /// Input buffers ([`Arg::Buf`]) are copied into freshly guest-`malloc`'d memory;
    /// output buffers ([`Arg::Out`]) and length out-params ([`Arg::OutLen`]) get guest
    /// allocations too. After the call, out-lengths are read back, each out-buffer's
    /// copy-back length is resolved (see [`OutLen`]), the bytes are copied into the
    /// caller's slices, and all allocations are freed (even on failure). Returns the
    /// first result widened to `i64` (`0` for a `void` return).
    ///
    /// The guest must export `guest_malloc(i32)->i32` / `guest_free(i32)`.
    pub fn call(&mut self, name: &str, args: &mut [Arg]) -> Result<i64> {
        // Phase 1: allocate + copy inputs in; build wasm params; remember each arg's
        // guest offset (for out-copy and freeing).
        let mut ptrs: Vec<Option<u32>> = vec![None; args.len()];
        let mut to_free: Vec<u32> = Vec::new();
        let mut params: Vec<Val> = Vec::with_capacity(args.len());
        for (i, a) in args.iter().enumerate() {
            match a {
                Arg::I32(v) => params.push(Val::I32(*v)),
                Arg::USize(v) => params.push(Val::I32(*v as i32)),
                Arg::Buf(bytes) => {
                    let ptr = self.copy_in(bytes)?;
                    ptrs[i] = Some(ptr);
                    to_free.push(ptr);
                    params.push(Val::I32(ptr as i32));
                }
                Arg::Out { dst, .. } => {
                    let ptr = self.guest_malloc(dst.len())?;
                    ptrs[i] = Some(ptr);
                    to_free.push(ptr);
                    params.push(Val::I32(ptr as i32));
                }
                Arg::OutLen(_) => {
                    let ptr = self.guest_malloc(SIZE_T_BYTES)?;
                    ptrs[i] = Some(ptr);
                    to_free.push(ptr);
                    params.push(Val::I32(ptr as i32));
                }
            }
        }

        // Phase 2: invoke.
        let func = self
            .instance
            .get_func(&mut self.store, name)
            .ok_or_else(|| anyhow!("exported function `{}` not found", name))?;
        let n_results = func.ty(&self.store).results().len();
        let mut results = vec![Val::null_func_ref(); n_results];
        let call_res = wasmtime_wasi::runtime::with_ambient_tokio_runtime(|| {
            func.call(&mut self.store, &params, &mut results)
        });
        if let Err(e) = call_res {
            for ptr in to_free {
                let _ = self.guest_free(ptr);
            }
            return Err(anyhow::Error::from(e)).with_context(|| format!("failed to call `{}`", name));
        }
        let ret: i64 = match results.first() {
            None => 0, // void
            Some(Val::I32(v)) => *v as i64,
            Some(Val::I64(v)) => *v,
            other => {
                for ptr in to_free {
                    let _ = self.guest_free(ptr);
                }
                return Err(anyhow!(
                    "expected an integer or void result from `{}`, got {:?}",
                    name,
                    other
                ));
            }
        };

        // Phase 3: read out-length params, and write them back into the caller's slots.
        let mut lengths: Vec<Option<usize>> = vec![None; args.len()];
        for i in 0..args.len() {
            if matches!(args[i], Arg::OutLen(_)) {
                lengths[i] = Some(self.read_u32(ptrs[i].unwrap())? as usize);
            }
        }
        for i in 0..args.len() {
            if let Arg::OutLen(dst) = &mut args[i] {
                **dst = lengths[i].unwrap();
            }
        }

        // Phase 4: copy each out-buffer back into the caller's slice.
        for i in 0..args.len() {
            let (ptr, spec, cap) = match &args[i] {
                Arg::Out { dst, len } => (ptrs[i].unwrap(), *len, dst.len()),
                _ => continue,
            };
            let want = match spec {
                OutLen::Cap => cap,
                OutLen::Ret => (ret as usize).min(cap),
                OutLen::FromArg(j) => lengths
                    .get(j)
                    .and_then(|l| *l)
                    .ok_or_else(|| anyhow!("OutLen::FromArg({j}) does not refer to an OutLen arg"))?
                    .min(cap),
                // C string: copy up to and including the NUL (if it fits).
                OutLen::Nul => (self.guest_cstr_len(ptr, cap)? + 1).min(cap),
            };
            let bytes = self.read_mem(ptr, want)?;
            if let Arg::Out { dst, .. } = &mut args[i] {
                dst[..bytes.len()].copy_from_slice(&bytes);
            }
        }

        // Phase 5: free.
        for ptr in to_free {
            let _ = self.guest_free(ptr);
        }
        Ok(ret)
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

    /// Write `bytes` into guest memory at offset `ptr` (bounds-checked).
    fn write_mem(&mut self, ptr: u32, bytes: &[u8]) -> Result<()> {
        // Fetch base+size on each access, so a memory growth is reflected.
        let (base, size) = self.guest_mem()?;
        let off = ptr as usize;
        let end = off
            .checked_add(bytes.len())
            .ok_or_else(|| anyhow!("guest write length overflow"))?;
        if end > size {
            return Err(anyhow!("guest write [{off}..{end}] exceeds memory size {size}"));
        }
        // SAFETY: `base` is the host address of the guest's linear memory (obtained the
        // same way the lind syscall layer obtains it), `[off, end)` is bounds-checked
        // against its current size, and `bytes` is a distinct host allocation.
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), base.add(off), bytes.len());
        }
        Ok(())
    }

    /// Read `len` bytes out of guest memory at offset `ptr` (bounds-checked).
    fn read_mem(&mut self, ptr: u32, len: usize) -> Result<Vec<u8>> {
        let (base, size) = self.guest_mem()?;
        let off = ptr as usize;
        let end = off
            .checked_add(len)
            .ok_or_else(|| anyhow!("guest read length overflow"))?;
        if end > size {
            return Err(anyhow!("guest read [{off}..{end}] exceeds memory size {size}"));
        }
        let mut out = vec![0u8; len];
        // SAFETY: as write_mem, with the destination a fresh host allocation of `len`.
        unsafe {
            std::ptr::copy_nonoverlapping(base.add(off), out.as_mut_ptr(), len);
        }
        Ok(out)
    }

    /// Read a wasm32 `size_t` (4-byte little-endian) out of guest memory.
    fn read_u32(&mut self, ptr: u32) -> Result<u32> {
        let b = self.read_mem(ptr, SIZE_T_BYTES)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// Length (excluding NUL) of a C string in guest memory, scanning at most `max`
    /// bytes; returns `max` if no NUL is found within the buffer.
    fn guest_cstr_len(&mut self, ptr: u32, max: usize) -> Result<usize> {
        let b = self.read_mem(ptr, max)?;
        Ok(b.iter().position(|&c| c == 0).unwrap_or(max))
    }

    /// Copy `bytes` into a fresh guest allocation; returns the guest offset.
    fn copy_in(&mut self, bytes: &[u8]) -> Result<u32> {
        let ptr = self.guest_malloc(bytes.len())?;
        self.write_mem(ptr, bytes)?;
        Ok(ptr)
    }
}

/// Size of a `size_t` in the wasm32 guest.
const SIZE_T_BYTES: usize = 4;
