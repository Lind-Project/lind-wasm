#![allow(dead_code)]

use anyhow::Result;
use cage::memory::check_addr_write;
use rand::Rng;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use sysdefs::constants::lind_platform_const;
use sysdefs::constants::lind_platform_const::{UNUSED_ARG, UNUSED_ID};
use sysdefs::constants::Errno;
use sysdefs::logging::lind_debug_panic;
use threei::threei::{
    copy_data_between_cages, copy_handler_table_to_cage, make_syscall, register_handler,
};
use threei::threei_const;
use typemap::path_conversion::get_cstr;
use wasmtime::{AsContext, AsContextMut, AsyncifyState, Caller};
use wasmtime_lind_dylink::DynamicLoader;
use wasmtime_lind_multi_process::{get_memory_base, get_memory_base_and_size, LindHost};
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
use sysdefs::constants::syscall_const::{
    CLONE_SYSCALL, EXEC_SYSCALL, EXIT_GROUP_SYSCALL, EXIT_SYSCALL,
};

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
                    if rewind_res > 0 {
                        // On Asyncify rewind replay, `clone` must not be executed again.
                        // The positive rewind result is the real child cage id returned to the
                        // parent guest by default. If a grate supplied a pending visible return
                        // value during the first normal execution, consume it here and return it
                        // instead. The child-side rewind result is 0 and must not be overridden.
                        if let Some(retval) = wasmtime_lind_multi_process::take_pending_clone_visible_retval(&mut caller) {
                            // Only override the return value with the pending visible retval if it is actually set.
                            // If it's not set, that means the clone syscall being replayed on rewind is the one in the child process, and we should return the actual syscall return value (0) instead of the pending visible retval from the parent process.
                            if retval > 0 {
                                return retval;
                            }
                        }
                    }
                    return rewind_res;
                }
            }

            // If we are reaching here at rewind state, that means fork was called within
            // a syscall-interrupted signal handler. We should restore the saved return value
            // of the syscall that was interrupted, rather than re-executing it.
            // If there's no syscall rewind data, we're rewinding from an exit_call —
            // let the rewind continue without re-executing the syscall.
            if let AsyncifyState::Rewind(_) = caller.as_context().get_asyncify_state() {
                let retval = match caller.as_context_mut().get_current_syscall_rewind_data() {
                    Some(v) => v,
                    None => {
                        wasmtime_lind_multi_process::signal::signal_handler(&mut caller);
                        return 0;
                    }
                };
                // let signal handler finish rest of the rewinding process
                wasmtime_lind_multi_process::signal::signal_handler(&mut caller);
                return retval;
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
                && matches!(
                    call_number as i32,
                    CLONE_SYSCALL | EXIT_SYSCALL | EXIT_GROUP_SYSCALL
                ) {
                wasmtime_lind_multi_process::current_tid(&mut caller) as u64
            } else {
                arg2
            };

            let retval = make_syscall(
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
            );

            if call_number as i32 == CLONE_SYSCALL {
                // Save the grate-defined return value produced by the normal clone execution. 
                // The guest-visible parent return is produced later during Asyncify rewind 
                // replay, so this value must be carried across that boundary.
                wasmtime_lind_multi_process::set_pending_clone_visible_retval(&mut caller, retval).unwrap_or_else(|e| {
                    panic!("{}", format!(
                        "failed to set pending_clone_visible_retval for retval={retval} with error: {e}"
                    ));
                });
            }

            // If the syscall was interrupted by a signal (EINTR), invoke the signal handler.
            // If fork is called within the signal handler, asyncify will unwind the stack;
            // we save the syscall return value so it can be restored on rewind.
            // Only negate `retval` if it falls within the valid errno range;
            // since negating `I32::MIN` would cause an overflow panic.
            if retval < 0 && retval > -256 && -retval == sysdefs::constants::Errno::EINTR as i32 {
                caller.as_context_mut().append_syscall_asyncify_data(retval);
                wasmtime_lind_multi_process::signal::signal_handler(&mut caller);

                if caller.as_context().get_asyncify_state() == AsyncifyState::Unwind {
                    return 0;
                } else {
                    caller.as_context_mut().pop_syscall_asyncify_data();
                }
            }

            retval
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
        move |mut caller: Caller<'_, T>| -> u64 { get_memory_base(&mut caller) },
    )?;

    linker.func_wrap(
        "lind",
        "lind-get-cage-id",
        move |mut caller: Caller<'_, T>| -> u64 {
            let cageid = wasmtime_lind_multi_process::current_cageid(&mut caller) as u64;
            cageid
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
        move |mut caller: Caller<'_, T>, ptr: i32| -> i32 {
            let mem_base = get_memory_base(&mut caller);
            if let Ok(msg) = get_cstr(mem_base + (ptr as u32) as u64) {
                eprintln!("[LIND DEBUG STR]: {}", msg);
            }
            ptr
        },
    )?;

    Ok(())
}

/// Register the 5 environ/args/random host functions under a given module name.
///
/// glibc's `_start()` imports these from `"lind"`, while Rust std compiled with
/// `wasm32-wasip1` imports them from `"wasi_snapshot_preview1"`. We call this
/// function twice to register under both module names, avoiding duplication.
fn add_environ_funcs_to_linker<
    T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
    U: Clone + Send + 'static + std::marker::Sync,
>(
    linker: &mut wasmtime::Linker<T>,
    module: &str,
    get_environ: impl Fn(&T) -> &LindEnviron + Send + Sync + Copy + 'static,
) -> anyhow::Result<()> {
    linker.func_wrap(
        module,
        "args_sizes_get",
        move |mut caller: Caller<'_, T>, ptr_argc: i32, ptr_buf_size: i32| -> i32 {
            let cx = get_environ(caller.data());
            let argc = cx.args.len() as u32;
            let buf_size: u32 = cx.args.iter().map(|a| a.len() as u32 + 1).sum();
            let cageid = wasmtime_lind_multi_process::current_cageid(&mut caller) as u64;
            let (base_u64, mem_size) = get_memory_base_and_size(&mut caller);
            let base = base_u64 as *mut u8;
            let off_argc = ptr_argc as u32 as usize;
            let off_buf_size = ptr_buf_size as u32 as usize;
            let size_of_u32 = std::mem::size_of::<u32>();
            if off_argc + size_of_u32 > mem_size || off_buf_size + size_of_u32 > mem_size {
                return Errno::EFAULT as i32;
            }
            if check_addr_write(cageid, base_u64 + off_argc as u64, size_of_u32).is_err()
                || check_addr_write(cageid, base_u64 + off_buf_size as u64, size_of_u32).is_err()
            {
                return Errno::EFAULT as i32;
            }
            unsafe {
                write_u32(base, off_argc, argc);
                write_u32(base, off_buf_size, buf_size);
            }
            0
        },
    )?;

    linker.func_wrap(
        module,
        "args_get",
        move |mut caller: Caller<'_, T>, argv_ptrs: i32, argv_buf: i32| -> i32 {
            let cx = get_environ(caller.data());
            let args: Vec<String> = cx.args.clone();
            let cageid = wasmtime_lind_multi_process::current_cageid(&mut caller) as u64;
            let (base_u64, mem_size) = get_memory_base_and_size(&mut caller);
            let base = base_u64 as *mut u8;
            let mut buf_offset = argv_buf as u32;
            let size_of_u32 = std::mem::size_of::<u32>();
            for (i, arg) in args.iter().enumerate() {
                let ptr_slot = argv_ptrs as u32 as usize + i * size_of_u32;
                let bytes = arg.as_bytes();
                let end = buf_offset as usize + bytes.len() + 1;
                if ptr_slot + size_of_u32 > mem_size || end > mem_size {
                    return Errno::EFAULT as i32;
                }
                if check_addr_write(cageid, base_u64 + ptr_slot as u64, size_of_u32).is_err()
                    || check_addr_write(cageid, base_u64 + buf_offset as u64, bytes.len() + 1)
                        .is_err()
                {
                    return Errno::EFAULT as i32;
                }
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
        module,
        "environ_sizes_get",
        move |mut caller: Caller<'_, T>, ptr_count: i32, ptr_buf_size: i32| -> i32 {
            let cx = get_environ(caller.data());
            let count = cx.env.len() as u32;
            let buf_size: u32 = cx
                .env
                .iter()
                .map(|(k, v)| k.len() as u32 + 1 + v.len() as u32 + 1)
                .sum();
            let cageid = wasmtime_lind_multi_process::current_cageid(&mut caller) as u64;
            let (base_u64, mem_size) = get_memory_base_and_size(&mut caller);
            let base = base_u64 as *mut u8;
            let off_count = ptr_count as u32 as usize;
            let off_buf_size = ptr_buf_size as u32 as usize;
            let size_of_u32 = std::mem::size_of::<u32>();
            if off_count + size_of_u32 > mem_size || off_buf_size + size_of_u32 > mem_size {
                return Errno::EFAULT as i32;
            }
            if check_addr_write(cageid, base_u64 + off_count as u64, size_of_u32).is_err()
                || check_addr_write(cageid, base_u64 + off_buf_size as u64, size_of_u32).is_err()
            {
                return Errno::EFAULT as i32;
            }
            unsafe {
                write_u32(base, off_count, count);
                write_u32(base, off_buf_size, buf_size);
            }
            0
        },
    )?;

    linker.func_wrap(
        module,
        "environ_get",
        move |mut caller: Caller<'_, T>, env_ptrs: i32, env_buf: i32| -> i32 {
            let cx = get_environ(caller.data());
            let env: Vec<(String, String)> = cx.env.clone();
            let cageid = wasmtime_lind_multi_process::current_cageid(&mut caller) as u64;
            let (base_u64, mem_size) = get_memory_base_and_size(&mut caller);
            let base = base_u64 as *mut u8;
            let mut buf_offset = env_buf as u32;
            let size_of_u32 = std::mem::size_of::<u32>();
            for (i, (key, val)) in env.iter().enumerate() {
                let ptr_slot = env_ptrs as u32 as usize + i * size_of_u32;
                let entry = format!("{}={}", key, val);
                let bytes = entry.as_bytes();
                let end = buf_offset as usize + bytes.len() + 1;
                if ptr_slot + size_of_u32 > mem_size || end > mem_size {
                    return Errno::EFAULT as i32;
                }
                if check_addr_write(cageid, base_u64 + ptr_slot as u64, size_of_u32).is_err()
                    || check_addr_write(cageid, base_u64 + buf_offset as u64, bytes.len() + 1)
                        .is_err()
                {
                    return Errno::EFAULT as i32;
                }
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
        module,
        "random_get",
        move |mut caller: Caller<'_, T>, buf: i32, buf_len: i32| -> i32 {
            let cageid = wasmtime_lind_multi_process::current_cageid(&mut caller) as u64;
            let (base_u64, mem_size) = get_memory_base_and_size(&mut caller);
            let base = base_u64 as *mut u8;
            let offset = buf as u32 as usize;
            let len = buf_len as u32 as usize;

            if offset + len > mem_size {
                return Errno::EFAULT as i32;
            }
            if check_addr_write(cageid, base_u64 + offset as u64, len).is_err() {
                return Errno::EFAULT as i32;
            }

            let mut bytes = vec![0u8; len];
            let mut rng = rand::thread_rng();
            rng.fill(&mut bytes[..]);

            unsafe {
                write_bytes(base, offset, &bytes);
            }
            0
        },
    )?;

    Ok(())
}

/// register dynamic loading related functions: dlopen, dlsym and dlclose
pub fn add_dylink_to_linker<
    T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
    U: Clone + Send + 'static + std::marker::Sync,
>(
    linker: &mut wasmtime::Linker<T>,
    dynamic_loader: Option<DynamicLoader<T>>,
) -> anyhow::Result<()> {
    let dylink_enabled = dynamic_loader.is_some();
    let cloned_dynamic_loader = dynamic_loader.clone();
    linker.func_wrap(
        "lind",
        "dlopen",
        move |mut caller: wasmtime::Caller<'_, T>, file: i32, mode: i32| -> i32 {
            if dylink_enabled {
                wasmtime_lind_dylink::dlopen_call(
                    &mut caller,
                    file,
                    mode,
                    cloned_dynamic_loader.clone().unwrap(),
                )
            } else {
                lind_debug_panic("dynamic loading support is not enabled!");
            }
        },
    )?;

    linker.func_wrap(
        "lind",
        "dlsym",
        move |mut caller: wasmtime::Caller<'_, T>, handle: i32, name: i32| -> i32 {
            if dylink_enabled {
                wasmtime_lind_dylink::dlsym_call(&mut caller, handle, name)
            } else {
                lind_debug_panic("dynamic loading support is not enabled!");
            }
        },
    )?;

    linker.func_wrap(
        "lind",
        "dlclose",
        move |mut caller: wasmtime::Caller<'_, T>, handle: i32| -> i32 {
            if dylink_enabled {
                wasmtime_lind_dylink::dlclose_call(&mut caller, handle)
            } else {
                lind_debug_panic("dynamic loading support is not enabled!");
            }
        },
    )?;

    Ok(())
}

/// Register all Lind host functions with the linker.
///
/// Groups:
/// - **syscall**: the unified `make-syscall` entry point
/// - **runtime**: memory base, cage ID, setjmp/longjmp, epoch callback, debug panic
/// - **dylink**: dlopen, dlsym and dlclose
/// - **debug** (lind_debug feature only): `lind_debug_num`, `lind_debug_str`
/// - **environ**: argv/environ/random_get under both `"lind"` and `"wasi_snapshot_preview1"`
pub fn add_to_linker<
    T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
    U: Clone + Send + 'static + std::marker::Sync,
>(
    linker: &mut wasmtime::Linker<T>,
    get_environ: impl Fn(&T) -> &LindEnviron + Send + Sync + Copy + 'static,
    dynamic_loader: Option<DynamicLoader<T>>,
) -> anyhow::Result<()> {
    add_syscall_to_linker(linker)?;
    add_runtime_to_linker(linker)?;
    add_dylink_to_linker(linker, dynamic_loader)?;
    #[cfg(feature = "lind_debug")]
    add_debug_to_linker(linker)?;
    add_environ_funcs_to_linker(linker, "lind", get_environ)?;
    add_environ_funcs_to_linker(linker, "wasi_snapshot_preview1", get_environ)?;
    Ok(())
}
