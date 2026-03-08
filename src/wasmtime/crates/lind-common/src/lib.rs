#![allow(dead_code)]

use anyhow::Result;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use sysdefs::constants::lind_platform_const;
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
// syscalls are passed directly to 3i's `make_syscall`.
//
// `UNUSED_ID` / `UNUSED_ARG` / `UNUSED_NAME` is a placeholder argument
// for functions that require a fixed number of parameters but do not utilize
// all of them.
use wasmtime_lind_utils::lind_syscall_numbers::{CLONE_SYSCALL, EXEC_SYSCALL, EXIT_SYSCALL};

/// Stores argv and environment variables for the guest program. During glibc's
/// `_start()`, the guest calls 4 imported host functions (`args_sizes_get`,
/// `args_get`, `environ_sizes_get`, `environ_get`) to retrieve argc/argv and
/// environ. This struct holds the data those functions serve.
#[derive(Clone, Default)]
pub struct LindEnviron {
    args: Vec<String>,
    env: Vec<(String, String)>,
}

impl LindEnviron {
    /// Build from program arguments and `--env` flags passed on the lind-boot
    /// command line. For `--env FOO=BAR`, the value is used directly. For
    /// `--env FOO` (no `=`), the value is inherited from the host process
    /// via `std::env::var`.
    pub fn new(args: &[String], vars: &[(String, Option<String>)]) -> Self {
        let env = vars
            .iter()
            .filter_map(|(key, val)| {
                let resolved = match val {
                    Some(v) => v.clone(),
                    None => std::env::var(key).ok()?,
                };
                Some((key.clone(), resolved))
            })
            .collect();
        Self {
            args: args.to_vec(),
            env,
        }
    }

    /// Clone args + env for a forked cage.
    pub fn fork(&self) -> Self {
        self.clone()
    }
}

/// Write a little-endian u32 at `base + offset` in guest linear memory.
unsafe fn write_u32(base: *mut u8, offset: usize, val: u32) {
    unsafe {
        std::ptr::copy_nonoverlapping(val.to_le_bytes().as_ptr(), base.add(offset), 4);
    }
}

/// Write `src` bytes at `base + offset` in guest linear memory.
unsafe fn write_bytes(base: *mut u8, offset: usize, src: &[u8]) {
    unsafe {
        std::ptr::copy_nonoverlapping(src.as_ptr(), base.add(offset), src.len());
    }
}

/// Register the `make-syscall` host function: the unified syscall entry point
/// from guest glibc into 3i.
fn add_syscall_to_linker<
    T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
    U: Clone + Send + 'static + std::marker::Sync,
>(
    linker: &mut wasmtime::Linker<T>,
) -> anyhow::Result<()> {
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
            let final_arg2 = if target_cageid == self_cageid
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
    Ok(())
}

/// Register runtime introspection functions: memory base address, cage ID,
/// setjmp/longjmp, epoch callback, and debug panic.
fn add_runtime_to_linker<
    T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
    U: Clone + Send + 'static + std::marker::Sync,
>(
    linker: &mut wasmtime::Linker<T>,
) -> anyhow::Result<()> {
    linker.func_wrap(
        "lind",
        "lind-get-memory-base",
        move |caller: Caller<'_, T>| -> u64 { get_memory_base(&caller) },
    )?;

    linker.func_wrap(
        "lind",
        "lind-get-cage-id",
        move |mut caller: Caller<'_, T>| -> u64 {
            wasmtime_lind_multi_process::current_cageid(&mut caller) as u64
        },
    )?;

    linker.func_wrap("lind", "debug-panic", move |str: u64| -> () {
        let _panic_str = unsafe { std::ffi::CStr::from_ptr(str as *const i8).to_str().unwrap() };
        sysdefs::logging::lind_debug_panic(format!("FROM GUEST: {}", _panic_str).as_str());
    })?;

    linker.func_wrap(
        "lind",
        "lind-setjmp",
        move |mut caller: Caller<'_, T>, jmp_buf: i32| -> i32 {
            wasmtime_lind_multi_process::setjmp_call(&mut caller, jmp_buf as u32)
        },
    )?;

    linker.func_wrap(
        "lind",
        "lind-longjmp",
        move |mut caller: Caller<'_, T>, jmp_buf: i32, retval: i32| -> i32 {
            wasmtime_lind_multi_process::longjmp_call(&mut caller, jmp_buf as u32, retval)
        },
    )?;

    linker.func_wrap(
        "lind",
        "epoch_callback",
        move |mut caller: Caller<'_, T>| {
            wasmtime_lind_multi_process::signal::signal_handler(&mut caller);
        },
    )?;

    linker.func_wrap(
        "lind",
        "random_get",
        move |caller: Caller<'_, T>, buf: i32, buf_len: i32| -> i32 {
            let base = get_memory_base(&caller) as *mut u8;
            let slice = unsafe {
                std::slice::from_raw_parts_mut(base.add(buf as usize), buf_len as usize)
            };
            let mut file = std::fs::File::open("/dev/urandom").unwrap();
            std::io::Read::read_exact(&mut file, slice).unwrap();
            0 // __WASI_ERRNO_SUCCESS
        },
    )?;

    Ok(())
}

/// Register debug-only host functions under the `"debug"` module.
#[cfg(feature = "lind_debug")]
fn add_debug_to_linker<
    T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
    U: Clone + Send + 'static + std::marker::Sync,
>(
    linker: &mut wasmtime::Linker<T>,
) -> anyhow::Result<()> {
    linker.func_wrap(
        "debug",
        "lind_debug_num",
        move |_caller: Caller<'_, T>, num: u32| -> u32 {
            eprintln!("[LIND DEBUG NUM]: {}", num);
            num
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
            ptr
        },
    )?;

    Ok(())
}

/// Register the 4 argv/environ host functions that glibc's `_start()` calls
/// to initialize `argc`, `argv`, and `environ` before entering `main()`.
fn add_environ_to_linker<
    T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
    U: Clone + Send + 'static + std::marker::Sync,
>(
    linker: &mut wasmtime::Linker<T>,
    get_environ: impl Fn(&T) -> &LindEnviron + Send + Sync + Copy + 'static,
) -> anyhow::Result<()> {
    linker.func_wrap(
        "lind",
        "args_sizes_get",
        move |caller: Caller<'_, T>, ptr_argc: i32, ptr_buf_size: i32| -> i32 {
            let cx = get_environ(caller.data());
            let argc = cx.args.len() as u32;
            let buf_size: u32 = cx.args.iter().map(|a| a.len() as u32 + 1).sum();
            let base = get_memory_base(&caller) as *mut u8;
            unsafe {
                write_u32(base, ptr_argc as usize, argc);
                write_u32(base, ptr_buf_size as usize, buf_size);
            }
            0
        },
    )?;

    linker.func_wrap(
        "lind",
        "args_get",
        move |caller: Caller<'_, T>, argv_ptrs: i32, argv_buf: i32| -> i32 {
            let cx = get_environ(caller.data());
            let args: Vec<String> = cx.args.clone();
            let base = get_memory_base(&caller) as *mut u8;
            let mut buf_offset = argv_buf as u32;
            for (i, arg) in args.iter().enumerate() {
                let ptr_slot = argv_ptrs as usize + i * 4;
                let bytes = arg.as_bytes();
                unsafe {
                    write_u32(base, ptr_slot, buf_offset);
                    write_bytes(base, buf_offset as usize, bytes);
                    *base.add(buf_offset as usize + bytes.len()) = 0;
                }
                buf_offset += bytes.len() as u32 + 1;
            }
            0
        },
    )?;

    linker.func_wrap(
        "lind",
        "environ_sizes_get",
        move |caller: Caller<'_, T>, ptr_count: i32, ptr_buf_size: i32| -> i32 {
            let cx = get_environ(caller.data());
            let count = cx.env.len() as u32;
            let buf_size: u32 = cx
                .env
                .iter()
                .map(|(k, v)| k.len() as u32 + 1 + v.len() as u32 + 1)
                .sum();
            let base = get_memory_base(&caller) as *mut u8;
            unsafe {
                write_u32(base, ptr_count as usize, count);
                write_u32(base, ptr_buf_size as usize, buf_size);
            }
            0
        },
    )?;

    linker.func_wrap(
        "lind",
        "environ_get",
        move |caller: Caller<'_, T>, env_ptrs: i32, env_buf: i32| -> i32 {
            let cx = get_environ(caller.data());
            let env: Vec<(String, String)> = cx.env.clone();
            let base = get_memory_base(&caller) as *mut u8;
            let mut buf_offset = env_buf as u32;
            for (i, (key, val)) in env.iter().enumerate() {
                let ptr_slot = env_ptrs as usize + i * 4;
                let entry = format!("{}={}", key, val);
                let bytes = entry.as_bytes();
                unsafe {
                    write_u32(base, ptr_slot, buf_offset);
                    write_bytes(base, buf_offset as usize, bytes);
                    *base.add(buf_offset as usize + bytes.len()) = 0;
                }
                buf_offset += bytes.len() as u32 + 1;
            }
            0
        },
    )?;

    Ok(())
}

/// Register WASI snapshot preview1 compatibility functions for Rust programs.
///
/// Rust std compiled with `wasm32-wasip1` imports `args_sizes_get`, `args_get`,
/// `environ_sizes_get`, `environ_get`, and `random_get` from the
/// `"wasi_snapshot_preview1"` module. We register these under that module name
/// so Rust binaries work out of the box without any crate patching.
fn add_wasi_compat_to_linker<
    T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
    U: Clone + Send + 'static + std::marker::Sync,
>(
    linker: &mut wasmtime::Linker<T>,
    get_environ: impl Fn(&T) -> &LindEnviron + Send + Sync + Copy + 'static,
) -> anyhow::Result<()> {
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "args_sizes_get",
        move |caller: Caller<'_, T>, ptr_argc: i32, ptr_buf_size: i32| -> i32 {
            let cx = get_environ(caller.data());
            let argc = cx.args.len() as u32;
            let buf_size: u32 = cx.args.iter().map(|a| a.len() as u32 + 1).sum();
            let base = get_memory_base(&caller) as *mut u8;
            unsafe {
                write_u32(base, ptr_argc as usize, argc);
                write_u32(base, ptr_buf_size as usize, buf_size);
            }
            0
        },
    )?;

    linker.func_wrap(
        "wasi_snapshot_preview1",
        "args_get",
        move |caller: Caller<'_, T>, argv_ptrs: i32, argv_buf: i32| -> i32 {
            let cx = get_environ(caller.data());
            let args: Vec<String> = cx.args.clone();
            let base = get_memory_base(&caller) as *mut u8;
            let mut buf_offset = argv_buf as u32;
            for (i, arg) in args.iter().enumerate() {
                let ptr_slot = argv_ptrs as usize + i * 4;
                let bytes = arg.as_bytes();
                unsafe {
                    write_u32(base, ptr_slot, buf_offset);
                    write_bytes(base, buf_offset as usize, bytes);
                    *base.add(buf_offset as usize + bytes.len()) = 0;
                }
                buf_offset += bytes.len() as u32 + 1;
            }
            0
        },
    )?;

    linker.func_wrap(
        "wasi_snapshot_preview1",
        "environ_sizes_get",
        move |caller: Caller<'_, T>, ptr_count: i32, ptr_buf_size: i32| -> i32 {
            let cx = get_environ(caller.data());
            let count = cx.env.len() as u32;
            let buf_size: u32 = cx
                .env
                .iter()
                .map(|(k, v)| k.len() as u32 + 1 + v.len() as u32 + 1)
                .sum();
            let base = get_memory_base(&caller) as *mut u8;
            unsafe {
                write_u32(base, ptr_count as usize, count);
                write_u32(base, ptr_buf_size as usize, buf_size);
            }
            0
        },
    )?;

    linker.func_wrap(
        "wasi_snapshot_preview1",
        "environ_get",
        move |caller: Caller<'_, T>, env_ptrs: i32, env_buf: i32| -> i32 {
            let cx = get_environ(caller.data());
            let env: Vec<(String, String)> = cx.env.clone();
            let base = get_memory_base(&caller) as *mut u8;
            let mut buf_offset = env_buf as u32;
            for (i, (key, val)) in env.iter().enumerate() {
                let ptr_slot = env_ptrs as usize + i * 4;
                let entry = format!("{}={}", key, val);
                let bytes = entry.as_bytes();
                unsafe {
                    write_u32(base, ptr_slot, buf_offset);
                    write_bytes(base, buf_offset as usize, bytes);
                    *base.add(buf_offset as usize + bytes.len()) = 0;
                }
                buf_offset += bytes.len() as u32 + 1;
            }
            0
        },
    )?;

    linker.func_wrap(
        "wasi_snapshot_preview1",
        "random_get",
        move |caller: Caller<'_, T>, buf: i32, buf_len: i32| -> i32 {
            let base = get_memory_base(&caller) as *mut u8;
            let slice = unsafe {
                std::slice::from_raw_parts_mut(base.add(buf as usize), buf_len as usize)
            };
            let mut file = std::fs::File::open("/dev/urandom").unwrap();
            std::io::Read::read_exact(&mut file, slice).unwrap();
            0
        },
    )?;

    Ok(())
}

/// Register all Lind host functions with the linker.
///
/// Groups:
/// - **syscall**: the unified `make-syscall` entry point
/// - **runtime**: memory base, cage ID, setjmp/longjmp, epoch callback, debug panic, random_get
/// - **debug** (lind_debug feature only): `lind_debug_num`, `lind_debug_str`
/// - **environ**: argv/environ functions for glibc `_start()`
/// - **wasi_compat**: WASI snapshot preview1 aliases for Rust std compatibility
pub fn add_to_linker<
    T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
    U: Clone + Send + 'static + std::marker::Sync,
>(
    linker: &mut wasmtime::Linker<T>,
    get_environ: impl Fn(&T) -> &LindEnviron + Send + Sync + Copy + 'static,
) -> anyhow::Result<()> {
    add_syscall_to_linker(linker)?;
    add_runtime_to_linker(linker)?;
    #[cfg(feature = "lind_debug")]
    add_debug_to_linker(linker)?;
    add_environ_to_linker(linker, get_environ)?;
    add_wasi_compat_to_linker(linker, get_environ)?;
    Ok(())
}
