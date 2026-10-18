//! Load-time synthesis of fpcast thunks.
//!
//! A module built with `--fpcast-emu` calls function pointers through the uniform ABI
//! `(i64 x N) -> i64`. A module built without it only exports native-typed functions, so an
//! fpcast importer of such a function has nothing it can call through a function pointer.
//! [`synthesize_fpcast_thunk`] generates the missing thunk at load time: a tiny wasm module
//! that imports the native function and exports a uniform-ABI wrapper around it. The argument
//! and result conversions mirror `toABI`/`fromABI` in binaryen's `FuncCastEmulation` pass, so
//! a synthesized thunk is interchangeable with a compiled one.
//!
//! Like the compiled thunks, the wrapper is stateless and calls the target exactly once, so it
//! is transparent to Asyncify unwinding/rewinding.

use crate::prelude::*;
use crate::runtime::vm::VMFuncRef;
use crate::{AsContextMut, Extern, Func, FuncType, Instance, Module, Result, ValType};
use core::ptr::NonNull;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use wasmtime_environ::VMSharedTypeIndex;

/// Number of `i64` parameters of the uniform fpcast ABI (binaryen `max-func-params`).
/// Updated from the thunk exports of fpcast modules as they are discovered.
static FPCAST_ARITY: AtomicU32 = AtomicU32::new(18);

pub(crate) fn fpcast_arity() -> usize {
    FPCAST_ARITY.load(Ordering::Relaxed) as usize
}

pub(crate) fn set_fpcast_arity(arity: usize) {
    FPCAST_ARITY.store(arity as u32, Ordering::Relaxed);
}

/// Compiled adapter modules, keyed by their wasm bytes (i.e. by target signature and arity).
static ADAPTERS: Mutex<Option<HashMap<Vec<u8>, Module>>> = Mutex::new(None);

fn leb_u32(out: &mut Vec<u8>, mut v: u32) {
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn section(out: &mut Vec<u8>, id: u8, body: &[u8]) {
    out.push(id);
    leb_u32(out, body.len() as u32);
    out.extend_from_slice(body);
}

fn val_code(ty: &ValType) -> Result<u8> {
    Ok(match ty {
        ValType::I32 => 0x7f,
        ValType::I64 => 0x7e,
        ValType::F32 => 0x7d,
        ValType::F64 => 0x7c,
        other => bail!("fpcast thunk: unsupported value type {other:?}"),
    })
}

/// Which way an adapter converts.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    /// The adapter has the uniform ABI type and wraps a native-typed target (a "thunk").
    NativeToUniform,
    /// The adapter has a native type and wraps a uniform-ABI target.
    UniformToNative,
}

fn emit_to_abi(body: &mut Vec<u8>, ty: &ValType) {
    match ty {
        ValType::I32 => body.push(0xad), // i64.extend_i32_u
        ValType::I64 => {}
        ValType::F32 => body.extend_from_slice(&[0xbc, 0xad]), // i32.reinterpret_f32, extend_u
        ValType::F64 => body.push(0xbd),                       // i64.reinterpret_f64
        _ => unreachable!("rejected by val_code"),
    }
}

fn emit_from_abi(body: &mut Vec<u8>, ty: &ValType) {
    match ty {
        ValType::I32 => body.push(0xa7), // i32.wrap_i64
        ValType::I64 => {}
        ValType::F32 => body.extend_from_slice(&[0xa7, 0xbe]), // wrap, f32.reinterpret_i32
        ValType::F64 => body.push(0xbf),                       // f64.reinterpret_i64
        _ => unreachable!("rejected by val_code"),
    }
}

/// Build the wasm bytes of an adapter between the native signature `params -> results` and
/// the uniform ABI of `arity` i64 parameters. The adapter imports its target as `e.f` and
/// exports itself as `thunk`.
fn adapter_bytes(
    params: &[ValType],
    results: &[ValType],
    arity: usize,
    dir: Direction,
) -> Result<Vec<u8>> {
    if params.len() > arity {
        bail!(
            "fpcast adapter: {} params exceeds the fpcast ABI arity {}",
            params.len(),
            arity
        );
    }
    if results.len() > 1 {
        bail!("fpcast adapter: multi-value results are unsupported");
    }
    for t in params.iter().chain(results.iter()) {
        val_code(t)?;
    }

    let mut native = vec![0x60];
    leb_u32(&mut native, params.len() as u32);
    for p in params {
        native.push(val_code(p)?);
    }
    leb_u32(&mut native, results.len() as u32);
    for r in results {
        native.push(val_code(r)?);
    }
    let mut uniform = vec![0x60];
    leb_u32(&mut uniform, arity as u32);
    uniform.extend(std::iter::repeat(0x7e).take(arity));
    uniform.extend_from_slice(&[0x01, 0x7e]);

    // type 0 = the imported target, type 1 = the adapter itself
    let (target_ty, adapter_ty) = match dir {
        Direction::NativeToUniform => (&native, &uniform),
        Direction::UniformToNative => (&uniform, &native),
    };

    let mut m = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
    let mut types = vec![0x02];
    types.extend_from_slice(target_ty);
    types.extend_from_slice(adapter_ty);
    section(&mut m, 1, &types);
    // (import "e" "f" (func (type 0)))
    section(&mut m, 2, &[0x01, 0x01, b'e', 0x01, b'f', 0x00, 0x00]);
    // one defined function of type 1
    section(&mut m, 3, &[0x01, 0x01]);
    // (export "thunk" (func 1))
    section(
        &mut m,
        7,
        &[0x01, 0x05, b't', b'h', b'u', b'n', b'k', 0x00, 0x01],
    );

    let mut body = vec![0x00]; // no locals
    match dir {
        Direction::NativeToUniform => {
            for (i, p) in params.iter().enumerate() {
                body.push(0x20); // local.get i
                leb_u32(&mut body, i as u32);
                emit_from_abi(&mut body, p);
            }
            body.extend_from_slice(&[0x10, 0x00]); // call 0
            match results.first() {
                None => body.extend_from_slice(&[0x42, 0x00]), // i64.const 0
                Some(r) => emit_to_abi(&mut body, r),
            }
        }
        Direction::UniformToNative => {
            for (i, p) in params.iter().enumerate() {
                body.push(0x20);
                leb_u32(&mut body, i as u32);
                emit_to_abi(&mut body, p);
            }
            for _ in params.len()..arity {
                body.extend_from_slice(&[0x42, 0x00]); // i64.const 0
            }
            body.extend_from_slice(&[0x10, 0x00]); // call 0
            match results.first() {
                None => body.push(0x1a), // drop
                Some(r) => emit_from_abi(&mut body, r),
            }
        }
    }
    body.push(0x0b); // end

    let mut code = vec![0x01];
    leb_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    section(&mut m, 10, &code);
    Ok(m)
}

/// Compile (or fetch from the cache) the adapter module for `bytes`.
fn adapter_module(engine: &crate::Engine, bytes: Vec<u8>) -> Result<Module> {
    let mut cache = ADAPTERS.lock().unwrap();
    let cache = cache.get_or_insert_with(HashMap::new);
    if let Some(m) = cache.get(&bytes) {
        return Ok(m.clone());
    }
    let m = Module::new(engine, &bytes)?;
    cache.insert(bytes, m.clone());
    Ok(m)
}

/// Create a uniform-ABI thunk for the native-typed `func`.
pub(crate) fn synthesize_fpcast_thunk(mut store: impl AsContextMut, func: Func) -> Result<Func> {
    let ty = func.ty(&store);
    let params: Vec<ValType> = ty.params().collect();
    let results: Vec<ValType> = ty.results().collect();
    let bytes = adapter_bytes(
        &params,
        &results,
        fpcast_arity(),
        Direction::NativeToUniform,
    )?;
    let module = adapter_module(store.as_context().engine(), bytes)?;
    let instance = Instance::new(&mut store, &module, &[Extern::Func(func)])?;
    instance
        .get_func(&mut store, "thunk")
        .ok_or_else(|| format_err!("fpcast thunk: adapter has no `thunk` export"))
}

fn is_uniform(params: &[ValType], results: &[ValType], arity: usize) -> bool {
    params.len() == arity
        && params.iter().all(|p| matches!(p, ValType::I64))
        && matches!(results, [ValType::I64])
}

/// lind-wasm: handler of the `call_indirect` signature-mismatch builtin.
///
/// `callee` is the `VMFuncRef` found in the table and `expected` the signature the call site
/// asked for. If exactly one of the two is the uniform fpcast ABI, returns a `VMFuncRef` of
/// the expected type that adapts the callee (cached per store); `Ok(None)` means the
/// mismatch is not an fpcast/native one and the call should trap as before.
pub(crate) fn adapt_mismatched_callee(
    store: &mut dyn crate::runtime::vm::VMStore,
    callee: NonNull<VMFuncRef>,
    expected: VMSharedTypeIndex,
) -> Result<Option<NonNull<VMFuncRef>>> {
    let key = callee.as_ptr() as usize;
    if let Some(hit) = store
        .store_opaque_mut()
        .fpcast_adapter(key, expected.bits())
    {
        return Ok(NonNull::new(hit as *mut VMFuncRef));
    }

    let engine = store.store_opaque_mut().engine().clone();
    // SAFETY: `callee` is a funcref loaded from a table of this store.
    let callee_index = unsafe { callee.as_ref().type_index };
    let callee_ty = FuncType::from_shared_type_index(&engine, callee_index);
    let expected_ty = FuncType::from_shared_type_index(&engine, expected);
    let (cp, cr): (Vec<_>, Vec<_>) = (callee_ty.params().collect(), callee_ty.results().collect());
    let (ep, er): (Vec<_>, Vec<_>) = (
        expected_ty.params().collect(),
        expected_ty.results().collect(),
    );
    let arity = fpcast_arity();

    let bytes = if is_uniform(&ep, &er, arity) && !is_uniform(&cp, &cr, arity) {
        // fpcast caller, native callee: wrap the callee in a thunk
        adapter_bytes(&cp, &cr, arity, Direction::NativeToUniform)?
    } else if is_uniform(&cp, &cr, arity) && !is_uniform(&ep, &er, arity) {
        // native caller, fpcast callee: wrap the thunk in a native-typed adapter
        adapter_bytes(&ep, &er, arity, Direction::UniformToNative)?
    } else {
        return Ok(None);
    };

    let module = adapter_module(&engine, bytes)?;
    let store_id = store.store_opaque_mut().id();
    // SAFETY: `callee` is a funcref owned by this store.
    let func = unsafe { Func::from_vm_func_ref(store_id, callee) };
    let instance = Instance::new_adapter_instance(store, &module, &[Extern::Func(func)])?;
    let opaque = store.store_opaque_mut();
    let Some(Extern::Func(adapter)) = instance.get_export_opaque(opaque, "thunk") else {
        bail!("fpcast adapter: no `thunk` export");
    };
    let adapter_ref = adapter.vm_func_ref(opaque);
    opaque.insert_fpcast_adapter(key, expected.bits(), adapter_ref.as_ptr() as usize);
    Ok(Some(adapter_ref))
}

/// Diagnostic counters: total `call_indirect`s (when compiled with `count_indirect_calls`) and
/// signature mismatches handled by the lenient path. Printed at process exit when
/// `LIND_INDIRECT_STATS` is set.
pub(crate) mod stats {
    use std::sync::Once;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TOTAL: AtomicU64 = AtomicU64::new(0);
    static MISMATCHED: AtomicU64 = AtomicU64::new(0);
    static REGISTER: Once = Once::new();

    extern "C" fn print_stats() {
        let total = TOTAL.load(Ordering::Relaxed);
        let mism = MISMATCHED.load(Ordering::Relaxed);
        eprintln!(
            "LIND_INDIRECT_STATS total_indirect_calls={total} mismatched={mism} crossing_pct={:.4}",
            if total == 0 {
                0.0
            } else {
                100.0 * mism as f64 / total as f64
            }
        );
    }

    fn register() {
        REGISTER.call_once(|| {
            if std::env::var_os("LIND_INDIRECT_STATS").is_some() {
                unsafe { libc::atexit(print_stats) };
            }
        });
    }

    pub(crate) fn count_total() {
        register();
        TOTAL.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn count_mismatch() {
        register();
        MISMATCHED.fetch_add(1, Ordering::Relaxed);
    }
}
