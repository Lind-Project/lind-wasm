#![allow(dead_code)]

use anyhow::{anyhow, Result};
use rawposix::safeposix::dispatcher::lind_syscall_api;
use wasmtime_environ::MemoryIndex;
use wasmtime_lind_multi_process::{get_memory_base, LindHost, clone_constants::CloneArgStruct};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier, Condvar, Mutex};
use wasmtime::{AsContext, AsContextMut, Caller, ExternType, Linker, Module, SharedMemory, Store, Val, Extern, OnCalledAction, RewindingReturn, StoreOpaque, InstanceId};

// lind-common serves as the main entry point when lind_syscall. Any syscalls made in glibc would reach here first,
// then the syscall would be dispatched into rawposix, or other crates under wasmtime, depending on the syscall, to perform its job

#[derive(Clone)]
// stores some attributes associated with current runnning wasm instance (i.e. cage)
// each cage has its own lind-common context 
pub struct LindCommonCtx {
    // process id attached to the lind-common context, should be same as cage id
    pid: i32,
    
    // next cage id, shared between all lind-common context instance (i.e. all cages)
    next_cageid: Arc<AtomicU64>,
}

impl LindCommonCtx {
    // create a new lind-common context, should only be called once for then entire runtime
    pub fn new(next_cageid: Arc<AtomicU64>) -> Result<Self> {
        // cage id starts from 1
        let pid = 1;
        Ok(Self { pid, next_cageid })
    }

    // create a new lind-common context with pid provided, used by exec syscall
    pub fn new_with_pid(pid: i32, next_cageid: Arc<AtomicU64>,
        ) -> Result<Self> {
        Ok(Self { pid, next_cageid })
    }

    // entry point for lind_syscall in glibc, dispatching syscalls to rawposix or wasmtime
    pub fn lind_syscall<T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync, U: Clone + Send + 'static + std::marker::Sync>
                        (&self, call_number: u32, call_name: u64, caller: &mut Caller<'_, T>, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64, arg6: u64) -> i32 {
        let start_address = get_memory_base(&caller);
        match call_number as i32 {
            // clone syscall
            171 => {
                let clone_args = unsafe { &mut *((arg1 + start_address) as *mut CloneArgStruct) };
                clone_args.child_tid += start_address;
                wasmtime_lind_multi_process::clone_syscall(caller, clone_args)
            }
            // exec syscall
            69 => {
                wasmtime_lind_multi_process::exec_syscall(caller, arg1 as i64, arg2 as i64, arg3 as i64)
            }
            // exit syscall
            30 => {
                wasmtime_lind_multi_process::exit_syscall(caller, arg1 as i32)
            }
            // other syscalls goes into rawposix
            _ => {
                lind_syscall_api(
                    self.pid as u64,
                    call_number,
                    call_name,
                    start_address,
                    arg1,
                    arg2,
                    arg3,
                    arg4,
                    arg5,
                    arg6,
                )
            }
        }
    }

    // get current process id/cageid
    // currently unused interface but may be useful in the future
    pub fn getpid(&self) -> i32 {
        self.pid
    }

    // return the next avaliable cageid (cageid increment sequentially)
    fn next_cage_id(&self) -> Option<u64> {
        match self
            .next_cageid
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| match v {
                ..=0x1ffffffe => Some(v + 1),
                _ => None,
            }) {
            Ok(v) => Some(v + 1),
            Err(_) => None,
        }
    }

    // fork a new lind-common context, used by clone syscall
    pub fn fork(&self) -> Self {
        // cageid is automatically incremented here
        let next_pid = self.next_cage_id().unwrap();

        let forked_ctx = Self {
            pid: next_pid as i32,
            next_cageid: self.next_cageid.clone(),
        };

        return forked_ctx;
    }
}

// function to expose the handler to wasm module
// linker: wasmtime's linker to link the imported function to the actual function definition
// get_cx: function to retrieve LindCommonCtx from caller
pub fn add_to_linker<T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync, U: Clone + Send + 'static + std::marker::Sync>(
    linker: &mut wasmtime::Linker<T>,
    get_cx: impl Fn(&T) -> &LindCommonCtx + Send + Sync + Copy + 'static,
) -> anyhow::Result<()> {
    // attach lind_syscall to wasmtime
    linker.func_wrap(
        "lind",
        "lind-syscall",
        move |mut caller: Caller<'_, T>, call_number: u32, call_name: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64, arg6: u64| -> i32 {
            let host = caller.data().clone();
            let ctx = get_cx(&host);

            ctx.lind_syscall(call_number, call_name, &mut caller, arg1, arg2, arg3, arg4, arg5, arg6)
        },
    )?;

    Ok(())
}
