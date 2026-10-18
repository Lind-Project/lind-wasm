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
use crate::{AsContextMut, Extern, Func, Instance, Module, Result, ValType};
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

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

/// Build the wasm bytes of an adapter around a function of type `params -> results`.
fn adapter_bytes(params: &[ValType], results: &[ValType], arity: usize) -> Result<Vec<u8>> {
    if params.len() > arity {
        bail!(
            "fpcast thunk: {} params exceeds the fpcast ABI arity {}",
            params.len(),
            arity
        );
    }
    if results.len() > 1 {
        bail!("fpcast thunk: multi-value results are unsupported");
    }

    let mut m = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];

    // type section: 0 = target signature, 1 = uniform ABI signature
    let mut types = vec![0x02, 0x60];
    leb_u32(&mut types, params.len() as u32);
    for p in params {
        types.push(val_code(p)?);
    }
    leb_u32(&mut types, results.len() as u32);
    for r in results {
        types.push(val_code(r)?);
    }
    types.push(0x60);
    leb_u32(&mut types, arity as u32);
    types.extend(std::iter::repeat(0x7e).take(arity));
    types.extend_from_slice(&[0x01, 0x7e]);
    section(&mut m, 1, &types);

    // import section: (import "e" "f" (func (type 0)))
    section(&mut m, 2, &[0x01, 0x01, b'e', 0x01, b'f', 0x00, 0x00]);

    // function section: one defined function of type 1
    section(&mut m, 3, &[0x01, 0x01]);

    // export section: (export "thunk" (func 1))
    section(
        &mut m,
        7,
        &[0x01, 0x05, b't', b'h', b'u', b'n', b'k', 0x00, 0x01],
    );

    // code section
    let mut body = vec![0x00]; // no locals
    for (i, p) in params.iter().enumerate() {
        body.push(0x20); // local.get i
        leb_u32(&mut body, i as u32);
        match p {
            ValType::I32 => body.push(0xa7), // i32.wrap_i64
            ValType::I64 => {}
            ValType::F32 => body.extend_from_slice(&[0xa7, 0xbe]), // wrap, f32.reinterpret_i32
            ValType::F64 => body.push(0xbf),                       // f64.reinterpret_i64
            _ => unreachable!("rejected by val_code"),
        }
    }
    body.extend_from_slice(&[0x10, 0x00]); // call 0
    match results.first() {
        None => body.extend_from_slice(&[0x42, 0x00]), // i64.const 0
        Some(ValType::I32) => body.push(0xad),         // i64.extend_i32_u
        Some(ValType::I64) => {}
        Some(ValType::F32) => body.extend_from_slice(&[0xbc, 0xad]), // i32.reinterpret_f32, extend_u
        Some(ValType::F64) => body.push(0xbd),                       // i64.reinterpret_f64
        _ => unreachable!("rejected by val_code"),
    }
    body.push(0x0b); // end

    let mut code = vec![0x01];
    leb_u32(&mut code, body.len() as u32);
    code.extend_from_slice(&body);
    section(&mut m, 10, &code);

    Ok(m)
}

/// Create a uniform-ABI thunk for the native-typed `func`.
pub(crate) fn synthesize_fpcast_thunk(mut store: impl AsContextMut, func: Func) -> Result<Func> {
    let ty = func.ty(&store);
    let params: Vec<ValType> = ty.params().collect();
    let results: Vec<ValType> = ty.results().collect();
    let bytes = adapter_bytes(&params, &results, fpcast_arity())?;

    let engine = store.as_context().engine().clone();
    let module = {
        let mut cache = ADAPTERS.lock().unwrap();
        let cache = cache.get_or_insert_with(HashMap::new);
        match cache.get(&bytes) {
            Some(m) => m.clone(),
            None => {
                let m = Module::new(&engine, &bytes)?;
                cache.insert(bytes, m.clone());
                m
            }
        }
    };

    let instance = Instance::new(&mut store, &module, &[Extern::Func(func)])?;
    instance
        .get_func(&mut store, "thunk")
        .ok_or_else(|| format_err!("fpcast thunk: adapter has no `thunk` export"))
}
