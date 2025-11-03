#![allow(dead_code)]

use anyhow::Result;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use sysdefs::constants::lind_platform_const::{UNUSED_ARG, UNUSED_ID};
use threei::threei::{
    copy_data_between_cages, copy_handler_table_to_cage, make_syscall, register_handler,
};
use wasmtime::Caller;
use wasmtime_lind_multi_process::{clone_constants::CloneArgStruct, get_memory_base, LindHost};
// These syscalls (`clone`, `exec`, `exit`, `fork`) require special handling
// inside Lind Wasmtime before delegating to RawPOSIX. For example, they may
// involve operations like setting up stack memory that must be performed
// at the Wasmtime layer. Therefore, in the unified syscall entry point of
// Wasmtime, these calls are routed to their dedicated logic, while other
// syscalls are passed directly to 3i’s `make_syscall`.
//
// `UNUSED_ID` / `UNUSED_ARG` / `UNUSED_NAME` is a placeholder argument
// for functions that require a fixed number of parameters but do not utilize
// all of them.
use wasmtime_lind_utils::lind_syscall_numbers::{CLONE_SYSCALL, EXEC_SYSCALL, EXIT_SYSCALL};

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
    pub fn new_with_pid(pid: i32, next_cageid: Arc<AtomicU64>) -> Result<Self> {
        Ok(Self { pid, next_cageid })
    }

    // entry point for lind_syscall in glibc, dispatching syscalls to rawposix or wasmtime
    pub fn lind_syscall<
        T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
        U: Clone + Send + 'static + std::marker::Sync,
    >(
        &self,
        call_number: u32,
        call_name: u64,
        caller: &mut Caller<'_, T>,
        arg1: u64,
        arg2: u64,
        arg3: u64,
        arg4: u64,
        arg5: u64,
        arg6: u64,
    ) -> i32 {
        let start_address = get_memory_base(&caller);
        // todo:
        // replacing the execution path by calling to 3i first
        match call_number as i32 {
            // clone syscall
            CLONE_SYSCALL => {
                let clone_args = unsafe { &mut *((arg1 + start_address) as *mut CloneArgStruct) };
                clone_args.child_tid += start_address;
                wasmtime_lind_multi_process::clone_syscall(caller, clone_args)
            }
            // exec syscall
            EXEC_SYSCALL => wasmtime_lind_multi_process::exec_syscall(
                caller,
                arg1 as i64,
                arg2 as i64,
                arg3 as i64,
            ),
            // exit syscall
            EXIT_SYSCALL => wasmtime_lind_multi_process::exit_syscall(caller, arg1 as i32),
            // other syscalls goes into rawposix
            _ => {
                make_syscall(
                    self.pid as u64,
                    call_number as u64,
                    call_name as u64,
                    self.pid as u64, // Set target_cageid same with self_cageid by defualt
                    arg1,
                    self.pid as u64,
                    arg2,
                    self.pid as u64,
                    arg3,
                    self.pid as u64,
                    arg4,
                    self.pid as u64,
                    arg5,
                    self.pid as u64,
                    arg6,
                    self.pid as u64,
                )
            }
        }
    }

    // setjmp call. This function needs to be handled within wasmtime, but it is not an actual syscall so we use a different routine from lind_syscall
    pub fn lind_setjmp<
        T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
        U: Clone + Send + 'static + std::marker::Sync,
    >(
        &self,
        caller: &mut Caller<'_, T>,
        jmp_buf: u32,
    ) -> i32 {
        wasmtime_lind_multi_process::setjmp_call(caller, jmp_buf)
    }

    // longjmp call. This function needs to be handled within wasmtime, but it is not an actual syscall so we use a different routine from lind_syscall
    pub fn lind_longjmp<
        T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
        U: Clone + Send + 'static + std::marker::Sync,
    >(
        &self,
        caller: &mut Caller<'_, T>,
        jmp_buf: u32,
        retval: i32,
    ) -> i32 {
        wasmtime_lind_multi_process::longjmp_call(caller, jmp_buf, retval)
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
pub fn add_to_linker<
    T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
    U: Clone + Send + 'static + std::marker::Sync,
>(
    linker: &mut wasmtime::Linker<T>,
    get_cx: impl Fn(&T) -> &LindCommonCtx + Send + Sync + Copy + 'static,
) -> anyhow::Result<()> {
    // attach lind_syscall to wasmtime
    linker.func_wrap(
        "lind",
        "lind-syscall",
        move |mut caller: Caller<'_, T>,
              call_number: u32,
              call_name: u64,
              arg1: u64,
              arg2: u64,
              arg3: u64,
              arg4: u64,
              arg5: u64,
              arg6: u64|
              -> i32 {
            let host = caller.data().clone();
            let ctx = get_cx(&host);

            let retval = ctx.lind_syscall(
                call_number,
                call_name,
                &mut caller,
                arg1,
                arg2,
                arg3,
                arg4,
                arg5,
                arg6,
            );

            // TODO: add a signal check here as Linux also has a signal check when transition from kernel to userspace
            // However, Asyncify management in this function should be carefully rethinking if adding signal check here

            retval
        },
    )?;

    // Registers grate-specific syscall-like host functions `register-syscall` / `cp-data-syscall` /
    // `copy_handler_table_to_cage` into the Wasmtime linker. This is part of the 3i (inter-cage
    // interposition) system, which allows user-level libc code (e.g., glibc) to perform cage-to-grate
    // syscall routing in a way that *resembles normal syscalls* from the user’s perspective.
    //
    // To maintain consistency with traditional syscall patterns, we expose 3i-related functions
    // using the same mechanism as `lind` syscalls. These functions are declared in glibc headers and
    // invoked like syscalls. At runtime, they are resolved via Wasmtime’s linker and routed to
    // closures here. This particular function allows a cage to register a handler function (by index)
    // for a specific syscall number, targeting a specific grate.
    //
    // The same trampoline mechanism used by `lind-syscall` is reused to simplify design and
    // reduce interface divergence between normal syscalls and 3i interposition calls.
    //
    // attach register_handler to wasmtime
    linker.func_wrap(
        "lind",
        "register-syscall",
        move |targetcage: u64,
              targetcallnum: u64,
              handlefunc_index_in_this_grate: u64,
              this_grate_id: u64|
              -> i32 {
            register_handler(
                UNUSED_ARG,
                targetcage,
                targetcallnum,
                UNUSED_ID,
                handlefunc_index_in_this_grate,
                this_grate_id,
                UNUSED_ARG,
                UNUSED_ID,
                UNUSED_ARG,
                UNUSED_ID,
                UNUSED_ARG,
                UNUSED_ID,
                UNUSED_ARG,
                UNUSED_ID,
            )
        },
    )?;

    // attach copy_data_between_cages to wasmtime
    linker.func_wrap(
        "lind",
        "cp-data-syscall",
        move |thiscage: u64,
              targetcage: u64,
              srcaddr: u64,
              srccage: u64,
              destaddr: u64,
              destcage: u64,
              len: u64,
              copytype: u64|
              -> i32 {
            copy_data_between_cages(
                thiscage, targetcage, srcaddr, srccage, destaddr, destcage, len, UNUSED_ID,
                copytype, UNUSED_ID, UNUSED_ARG, UNUSED_ID, UNUSED_ARG, UNUSED_ID,
            ) as i32
        },
    )?;

    // attach copy_handler_table_to_cage to wasmtime
    linker.func_wrap(
        "lind",
        "copy_handler_table_to_cage",
        move |thiscage: u64, targetcage: u64| -> i32 {
            copy_handler_table_to_cage(
                UNUSED_ARG, thiscage, targetcage, UNUSED_ID, UNUSED_ARG, UNUSED_ID, UNUSED_ARG,
                UNUSED_ID, UNUSED_ARG, UNUSED_ID, UNUSED_ARG, UNUSED_ID, UNUSED_ARG, UNUSED_ID,
            ) as i32
        },
    )?;

    // export lind-get-memory-base for libc to query base address
    linker.func_wrap(
        "lind",
        "lind-get-memory-base",
        move |caller: Caller<'_, T>| -> u64 {
            // Return the base address of memory[0] for the calling instance
            let base = get_memory_base(&caller);
            base
        },
    )?;

    // export lind-get-cage-id for libc to query the current cage id (pid)
    linker.func_wrap(
        "lind",
        "lind-get-cage-id",
        move |caller: Caller<'_, T>| -> u64 {
            let host = caller.data().clone();
            let ctx = get_cx(&host);
            ctx.getpid() as u64
        },
    )?;

    // attach setjmp to wasmtime
    linker.func_wrap(
        "lind",
        "lind-setjmp",
        move |mut caller: Caller<'_, T>, jmp_buf: i32| -> i32 {
            let host = caller.data().clone();
            let ctx = get_cx(&host);

            ctx.lind_setjmp(&mut caller, jmp_buf as u32)
        },
    )?;

    // attach longjmp to wasmtime
    linker.func_wrap(
        "lind",
        "lind-longjmp",
        move |mut caller: Caller<'_, T>, jmp_buf: i32, retval: i32| -> i32 {
            let host = caller.data().clone();
            let ctx = get_cx(&host);

            ctx.lind_longjmp(&mut caller, jmp_buf as u32, retval)
        },
    )?;

    // epoch callback function
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "epoch_callback",
        move |mut caller: Caller<'_, T>| {
            wasmtime_lind_multi_process::signal::signal_handler(&mut caller);
        },
    )?;

    Ok(())
}