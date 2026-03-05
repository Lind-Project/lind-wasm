#![allow(dead_code)]

use std::sync::Arc;

use anyhow::Result;
use sysdefs::{constants::{RTLD_DEFAULT, RTLD_NEXT}, logging::lind_debug_panic};
use wasmtime::Caller;
use wasmtime_lind_multi_process::{LindHost, get_memory_base};

/// Type alias for the dynamic loader callback used by `dlopen`.
///
/// The loader is injected from the runtime and is responsible for actually
/// loading and instantiating the requested library. It receives:
/// - a mutable Caller (to access runtime state),
/// - the library path,
/// - the dlopen mode flags.
///
/// It returns an integer handle identifying the loaded library.
pub type DynamicLoader<T> =
    Arc<dyn for<'a> Fn(&'a mut wasmtime::Caller<'_, T>, &str, i32) -> i32 + Send + Sync + 'static>;

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
    loader: DynamicLoader<T>
) -> i32
{
    let base = get_memory_base(&mut caller);
    let path = typemap::get_cstr(base + file as u64).unwrap();

    // Delegate to runtime dynamic loader.
    loader(&mut caller, path, mode)
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
    let symbol = typemap::get_cstr(base + sym as u64).unwrap();

    // Resolve symbol based on handle semantics.
    let val = if handle == RTLD_DEFAULT {
        caller.find_library_symbol_from_global(symbol).unwrap()
    } else if handle == RTLD_NEXT {
        lind_debug_panic("[lind-dylink] dlsym RTLD_NEXT encountered but not supported");
    } else {
        caller.find_library_symbol_from_local(handle, symbol).unwrap()
    };
    // println!("[debug] dlsym resolves {} to {}", symbol, val);
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
