#![allow(dead_code)]

use anyhow::Result;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use sysdefs::constants::lind_platform_const::{UNUSED_ARG, UNUSED_ID};
use threei::threei::{
    copy_data_between_cages, copy_handler_table_to_cage, make_syscall, register_handler,
};
use threei::threei_const;
use typemap::path_conversion::get_cstr;
use wasmtime::Caller;
use wasmtime_lind_multi_process::{get_memory_base, LindHost};
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

// function to expose the handler to wasm module
// linker: wasmtime's linker to link the imported function to the actual function definition
pub fn add_to_linker<
    T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
    U: Clone + Send + 'static + std::marker::Sync,
>(
    linker: &mut wasmtime::Linker<T>,
) -> anyhow::Result<()> {
    // attach make_syscall to wasmtime
    linker.func_wrap(
        "lind",
        "make-syscall",
        move |mut caller: Caller<'_, T>,
              call_number: u32,
              call_name: u64,
              self_cageid: u64,
              target_cageid: u64,
              arg1: u64,
              arg1cageid: u64,
              arg2: u64,
              arg2cageid: u64,
              arg3: u64,
              arg3cageid: u64,
              arg4: u64,
              arg4cageid: u64,
              arg5: u64,
              arg5cageid: u64,
              arg6: u64,
              arg6cageid: u64|
              -> i32 {
            // TODO:
            // 1. add a signal check here as Linux also has a signal check when transition from kernel to userspace
            // However, Asyncify management in this function should be carefully rethinking if adding signal check here

            // With Asyncify enabled, an unwind/rewind resumes Wasmtime execution by re-entering
            // the original call site. This means the same hostcall/trampoline path can be
            // executed multiple times while representing a *single* logical operation.
            //
            // `clone` is particularly sensitive here: during a logical `clone`, the lind
            // trampoline can be re-entered multiple times (e.g., 3 times) after unwind/rewind.
            // If we forward the syscall to RawPOSIX on every re-entry, we will perform the
            // operation multiple times.
            //
            // In lind-boot we forward syscalls directly to RawPOSIX, so we replicate the state
            // check here to early-return when we are on a rewind replay path.
            if call_number as i32 == CLONE_SYSCALL {
                if let Some(rewind_res) = wasmtime_lind_multi_process::catch_rewind(&mut caller) {
                    return rewind_res;
                }
            }

            // Some thread-related operations must be executed against a specific thread's
            // VMContext (e.g., pthread_create/exit). Because syscalls may be interposed/routed
            // through 3i functionality and the effective thread instance cannot be reliably derived
            // from self/target cage IDs or per-argument cage IDs, we explicitly attach the *current*
            // source thread id (tid) for selected syscalls. (Note: `self_cageid == target_cageid` means
            // the syscall executes from cage)
            //
            // Concretely, for CLONE/EXEC we override arg2 with the current tid so that, when the call back
            // to wasmtime, it can resolve the correct thread instance deterministically, independent of
            // interposition or cross-cage routing.
            let final_arg2 = if self_cageid == target_cageid
                && matches!(call_number as i32, CLONE_SYSCALL | EXIT_SYSCALL)
            {
                wasmtime_lind_multi_process::current_tid(&mut caller) as u64
            } else {
                arg2
            };

            make_syscall(
                self_cageid,
                call_number as u64,
                call_name,
                target_cageid,
                arg1,
                arg1cageid,
                final_arg2,
                arg2cageid,
                arg3,
                arg3cageid,
                arg4,
                arg4cageid,
                arg5,
                arg5cageid,
                arg6,
                arg6cageid,
            )
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
        move |mut caller: Caller<'_, T>| -> u64 {
            // Return the base address of memory[0] for the calling instance
            let base = get_memory_base(&mut caller);
            base
        },
    )?;

    // export lind-get-cage-id for libc to query the current cage id
    linker.func_wrap(
        "lind",
        "lind-get-cage-id",
        move |mut caller: Caller<'_, T>| -> u64 {
            wasmtime_lind_multi_process::current_cageid(&mut caller) as u64
        },
    )?;

    // attach lind-debug-panic to wasmtime
    linker.func_wrap("lind", "debug-panic", move |str: u64| -> () {
        let _panic_str = unsafe { std::ffi::CStr::from_ptr(str as *const i8).to_str().unwrap() };

        sysdefs::logging::lind_debug_panic(format!("FROM GUEST: {}", _panic_str).as_str());
    })?;

    // attach setjmp to wasmtime
    linker.func_wrap(
        "lind",
        "lind-setjmp",
        move |mut caller: Caller<'_, T>, jmp_buf: i32| -> i32 {
            wasmtime_lind_multi_process::setjmp_call(&mut caller, jmp_buf as u32)
        },
    )?;

    // attach longjmp to wasmtime
    linker.func_wrap(
        "lind",
        "lind-longjmp",
        move |mut caller: Caller<'_, T>, jmp_buf: i32, retval: i32| -> i32 {
            wasmtime_lind_multi_process::longjmp_call(&mut caller, jmp_buf as u32, retval)
        },
    )?;

    // epoch callback function
    linker.func_wrap(
        "lind",
        "epoch_callback",
        move |mut caller: Caller<'_, T>| {
            wasmtime_lind_multi_process::signal::signal_handler(&mut caller);
        },
    )?;

    #[cfg(feature = "lind_debug")]
    {
        linker.func_wrap(
            "debug",
            "lind_debug_num",
            move |_caller: Caller<'_, T>, num: u32| -> u32 {
                eprintln!("[LIND DEBUG NUM]: {}", num);
                num // Return the value to the WASM stack
            },
        )?;

        linker.func_wrap(
            "debug",
            "lind_debug_str",
            move |caller: Caller<'_, T>, ptr: i32| -> i32 {
                let mem_base = get_memory_base(&caller);
                if let Ok(msg) = get_cstr(mem_base + (ptr as u32) as u64) {
                    eprintln!("[LIND DEBUG STR]: {}", msg);
                }
                ptr // Return the pointer to the WASM stack
            },
        )?;
    }
    Ok(())
}
