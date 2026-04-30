#![allow(dead_code)]

use std::sync::Arc;

use anyhow::Result;
use sysdefs::{
    constants::{DylinkErrorCode, RTLD_DEFAULT, RTLD_NEXT},
    logging::lind_debug_panic,
};
use wasmtime::Caller;
use wasmtime_lind_multi_process::{get_memory_base, LindHost};

/// Type alias for the dynamic loader callback used by `dlopen`.
///
/// The loader is injected from the runtime and is responsible for actually
/// loading and instantiating the requested library. It receives:
/// - a mutable Caller (to access runtime state),
/// - the library path,
/// - the dlopen mode flags.
///
/// It returns an integer handle identifying the loaded library.
pub type DynamicLoader<T> = Arc<
    dyn for<'a> Fn(&'a mut wasmtime::Caller<'_, T>, i32, &str, i32) -> i32 + Send + Sync + 'static,
>;

/// Host implementation of `dlopen`.
///
/// This function is invoked from Wasm code. It translates the raw
/// pointer (offset into linear memory) into a host string path,
/// then delegates the actual loading to the provided `loader`
/// callback.
///
/// Returns a library handle on success.
pub fn dlopen_call<
    T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
    U: Clone + Send + 'static + std::marker::Sync,
>(
    mut caller: &mut Caller<'_, T>,
    file: i32,
    mode: i32,
    loader: DynamicLoader<T>,
) -> i32 {
    let base = get_memory_base(&mut caller);
    let path = match typemap::get_cstr(base + (file as u32) as u64) {
        Ok(path) => path,
        Err(_) => return -(DylinkErrorCode::EOPEN as i32),
    };

    println!("[debug] dlopen: {:?}", path);

    // retrieve the cageid of the caller
    let host = caller.data().clone();
    let ctx = host.get_ctx();
    let cageid = ctx.cageid;

    // Delegate to runtime dynamic loader.
    loader(&mut caller, cageid, path, mode)
}

/// Host implementation of `dlsym`.
///
/// This resolves a symbol name from either:
/// - the global namespace (RTLD_DEFAULT),
/// - the next object in search order (RTLD_NEXT, currently unsupported),
/// - or a specific library identified by `handle`.
///
/// Returns the resolved symbol value (e.g., function index or address).
pub fn dlsym_call<
    T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
    U: Clone + Send + 'static + std::marker::Sync,
>(
    mut caller: &mut Caller<'_, T>,
    handle: i32,
    sym: i32,
) -> i32 {
    let base = get_memory_base(&mut caller);
    let symbol = match typemap::get_cstr(base + (sym as u32) as u64) {
        Ok(path) => path,
        Err(_) => return -(DylinkErrorCode::ENOFOUND as i32),
    };

    println!("[debug] dlsym: {:?}", symbol);
    // Resolve symbol based on handle semantics.
    let val = if handle == RTLD_DEFAULT {
        match caller.find_library_symbol_from_global(symbol) {
            Some(val) => val,
            None => return -(DylinkErrorCode::ENOFOUND as i32),
        }
    } else if handle == RTLD_NEXT {
        lind_debug_panic("[lind-dylink] dlsym RTLD_NEXT encountered but not currently supported");
    } else {
        match caller.find_library_symbol_from_local(handle, symbol) {
            Some(val) => val,
            None => return -(DylinkErrorCode::ENOFOUND as i32),
        }
    };
    #[cfg(feature = "debug-dylink")]
    println!("[debug] dlsym resolves {} to {}", symbol, val);
    val as i32
}

/// Host implementation of `dlclose`.
///
/// This decreases the reference count of the library identified
/// by `handle`. If the reference count reaches zero and the library
/// is deletable, it will be removed from the symbol table.
///
/// Returns 0 on success (POSIX-compatible convention).
pub fn dlclose_call<
    T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
    U: Clone + Send + 'static + std::marker::Sync,
>(
    caller: &mut Caller<'_, T>,
    handle: i32,
) -> i32 {
    // Detach library from runtime (refcount decrement / possible unload).
    caller.detach_library(handle);

    0
}
