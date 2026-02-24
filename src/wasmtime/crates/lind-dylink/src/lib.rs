#![allow(dead_code)]

use std::sync::Arc;

use anyhow::Result;
use wasmtime::Caller;
use wasmtime_lind_multi_process::{LindHost, get_memory_base};

pub type DynamicLoader<T> =
    Arc<dyn for<'a> Fn(&'a mut wasmtime::Caller<'_, T>, &str) -> i32 + Send + Sync + 'static>;

pub fn dlopen_call<
    T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
    U: Clone + Send + 'static + std::marker::Sync,
>(
    mut caller: &mut Caller<'_, T>,
    lib: i32,
    loader: DynamicLoader<T>
) -> i32
{
    let base = get_memory_base(&mut caller);
    let path = typemap::get_cstr(base + lib as u64).unwrap();
    println!("[debug] dlopen path \"{}\"", path);

    loader(&mut caller, path)
}

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
    println!("[debug] dlsym {}", symbol);
    let lib_symbol = caller.get_library_symbols((handle - 1) as usize).unwrap();
    let val = *lib_symbol.get(&String::from(symbol)).unwrap() as i32;
    println!("[debug] dlsym resolves {} to {}", symbol, val);
    val
}

pub fn dlclose_call<
    T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
    U: Clone + Send + 'static + std::marker::Sync,
>(
    caller: &mut Caller<'_, T>,
    handle: i32,
) -> i32 {
    println!("[debug] dlclose handle {}", handle);
    // to do: implement dlclose
    0
}
