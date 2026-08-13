




use crate::cli::CliOptions;
use crate::lind_mpk::syscalls::{
    ENABLE_INTERPOSE_PTR, LIND_MANAGER,
    mpk_clone_syscall_entry, mpk_exit_syscall_entry
};
use crate::lind_mpk::RuntimeInfo::MPKRuntimeInfo;
use crate::shims::SyscallRuntime;
use anyhow::{Context, bail};
use cage::{VmmapBitWidth, get_cage};
use libc::{c_char, c_int, c_ulong, c_void};
use std::sync::atomic::Ordering;
use std::env;
use std::ffi::{CStr, CString};
use sysdefs::constants::syscall_const::{CLONE3_SYSCALL, EXEC_SYSCALL, EXIT_SYSCALL};
/// Minimal reproduction of the `link_map` struct from `<link.h>`.
/// The libc crate does not expose this type, so we define only the fields we
/// actually need. The layout matches the glibc ABI on x86-64 Linux.
#[repr(C)]
struct LinkMap {
    l_addr: c_ulong,
    l_name: *const c_char,
    l_ld: *mut c_void,
    l_next: *mut LinkMap,
    l_prev: *mut LinkMap,
}
use std::sync::Arc;
use wasmtime_lind_utils::LindCageManager;
use sysdefs::constants::lind_platform_const::{UNUSED_ID,  UNUSED_ARG, WASMTIME_CAGEID, RAWPOSIX_CAGEID};
use wasmtime_lind_multi_process::CAGE_START_ID;
use threei::threei_const;

// Import the type alias from RuntimeInfo module
use crate::lind_mpk::RuntimeInfo::EnableInterposeF;

// dlinfo request codes not yet exposed by the libc crate.
const RTLD_DI_LMID: c_int = 1;
const RTLD_DI_LINKMAP: c_int = 2;

// 4 GB virtual address space reserved for each cage with MAP_NORESERVE
// (no swap space is committed until pages are actually touched).
const MPK_MEMORY_SIZE: usize = 4 * 1024 * 1024 * 1024;

fn mpk_debug_enabled() -> bool {
    env::var_os("LIND_MPK_DEBUG").is_some()
}

fn mpk_debug(message: impl AsRef<str>) {
    if mpk_debug_enabled() {
        eprintln!("[lind-mpk] {}", message.as_ref());
    }
}

// ── MPK SyscallRuntime implementation ────────────────────────────────────────

/// MPK runtime implementation.
pub struct MpkRuntime;

impl SyscallRuntime for MpkRuntime {
    fn handle_clone(
        &self,
        cageid: u64,
        arg1: u64, arg1_cageid: u64,
        arg2: u64, arg2_cageid: u64,
        arg3: u64, arg3_cageid: u64,
        arg4: u64, arg4_cageid: u64,
        arg5: u64, arg5_cageid: u64,
        arg6: u64, arg6_cageid: u64,
    ) -> i32 {
        mpk_clone_syscall_entry(
            cageid,
            arg1, arg1_cageid,
            arg2, arg2_cageid,
            arg3, arg3_cageid,
            arg4, arg4_cageid,
            arg5, arg5_cageid,
            arg6, arg6_cageid,
        )
    }

    fn handle_exec(
        &self,
        cageid: u64,
        arg1: u64, arg1_cageid: u64,
        arg2: u64, arg2_cageid: u64,
        arg3: u64, arg3_cageid: u64,
        arg4: u64, arg4_cageid: u64,
        arg5: u64, arg5_cageid: u64,
        arg6: u64, arg6_cageid: u64,
    ) -> i32 {
        // arg1 = path, arg1_cageid = execing_cageid
        // arg2 = argv, arg2_cageid = envp (based on 3i convention)

        let _ = (cageid, arg4, arg4_cageid, arg5, arg5_cageid, arg6, arg6_cageid);
        
        let path_ptr = arg1 as *const c_char;
        let path_ptr_cageid = arg1_cageid;
        let argv_ptr = arg2 as *const *const c_char;
        let argv_ptr_cageid = arg2_cageid;
        let envp_ptr = arg3 as *const *const c_char;
        let envp_ptr_cageid = arg3_cageid;

        //TODO: handle argument gathering from different cages
        //cageid is the rawposix cageid
        match exec_mpk_internal(arg1_cageid, path_ptr, argv_ptr, envp_ptr) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("[lind-mpk] exec failed: {}", e);
                -1
            }
        }
    }

    fn handle_exit(
        &self,
        cageid: u64,
        arg1: u64, arg1_cageid: u64,
        arg2: u64, arg2_cageid: u64,
        arg3: u64, arg3_cageid: u64,
        arg4: u64, arg4_cageid: u64,
        arg5: u64, arg5_cageid: u64,
        arg6: u64, arg6_cageid: u64,
    ) -> i32 {
        mpk_exit_syscall_entry(
            cageid,
            arg1, arg1_cageid,
            arg2, arg2_cageid,
            arg3, arg3_cageid,
            arg4, arg4_cageid,
            arg5, arg5_cageid,
            arg6, arg6_cageid,
        )
    }
}

// ── MPK syscall interposition and execution ──────────────────────────────────

/// Syscall interposition handler: forwards every native syscall issued inside
/// the isolated dlmopen namespace through 3i's dispatch table so it reaches
/// RawPOSIX for sandboxed handling, exactly like a Wasm cage.
///
/// This function is registered with the custom glibc via
/// `__enable_syscall_interpose`. Once registered, any libc-level syscall made
/// by the guest .so calls this handler instead of entering the kernel directly.
extern "C" fn lind_syscall_handler(
    number: i64,
    a1: i64,
    a2: i64,
    a3: i64,
    a4: i64,
    a5: i64,
    a6: i64,
    _nargs: i32,
) -> i64 {
    threei::make_syscall(
        CAGE_START_ID as u64, // self_cageid
        number as u64,
        0,                    // _syscall_name: unused for native
        CAGE_START_ID as u64, // target_cageid
        a1 as u64,
        CAGE_START_ID as u64,
        a2 as u64,
        CAGE_START_ID as u64,
        a3 as u64,
        CAGE_START_ID as u64,
        a4 as u64,
        CAGE_START_ID as u64,
        a5 as u64,
        CAGE_START_ID as u64,
        a6 as u64,
        CAGE_START_ID as u64,
    )
}

pub fn init_mpk(lind_manager: Arc<LindCageManager>) {
    mpk_debug("initializing lind-mpk");
    // Publish the manager globally so mpk_clone_syscall_entry can reach it.
    LIND_MANAGER.set(lind_manager).ok();
    mpk_debug("lind-mpk initialized successfully");
}


/// Internal helper for MPK exec: tears down the old namespace and loads a new program.
///
/// This is called by MpkRuntime::handle_exec when the guest issues an exec syscall.
/// It performs the following steps:
/// 1. Retrieves and tears down the existing MPKRuntimeInfo (closes dlmopen handles)
/// 2. Parses the path, argv, and envp from the native pointers
/// 3. Loads and executes the new .so using the same logic as execute_mpk
fn exec_mpk_internal(
    cage_id: u64,
    path_ptr: *const c_char,
    argv_ptr: *const *const c_char,
    envp_ptr: *const *const c_char
) -> anyhow::Result<i32> {
    mpk_debug(format!("exec_mpk_internal for cage {}", cage_id));

    // Step 1: Parse all arguments into owned Rust values BEFORE any teardown.
    // The pointers live in the old namespace's memory; dlclose would invalidate them.

    let so_path = unsafe { CStr::from_ptr(path_ptr) }
        .to_str()
        .context("invalid UTF-8 in path")?
        .to_string();

    //step 1.1: locate the .so file, turn it into fully qualified path for dlmopen
    let canonical_so_path = std::fs::canonicalize(&so_path)
        .context("failed to canonicalize .so path")?;
    let so_path = canonical_so_path
        .to_str()
        .context("invalid UTF-8 in .so path")?
        .to_owned();

    let c_so_path = CString::new(so_path.as_str()).context("NUL byte in .so path")?;

    mpk_debug(format!("executing new program: {}", so_path));

    let mut args = Vec::new();
    if !argv_ptr.is_null() {
        let mut i = 0;
        loop {
            let arg_ptr = unsafe { *argv_ptr.offset(i) };
            if arg_ptr.is_null() {
                break;
            }
            let arg_str = unsafe { CStr::from_ptr(arg_ptr) }
                .to_str()
                .context("invalid UTF-8 in argv")?
                .to_string();
            args.push(arg_str);
            i += 1;
        }
    }

    let mut vars = Vec::new();
    if !envp_ptr.is_null() {
        let mut i = 0;
        loop {
            let env_ptr = unsafe { *envp_ptr.offset(i) };
            if env_ptr.is_null() {
                break;
            }
            let env_str = unsafe { CStr::from_ptr(env_ptr) }
                .to_str()
                .context("invalid UTF-8 in envp")?;

            // Split "KEY=VALUE" into (KEY, Some(VALUE))
            if let Some((key, val)) = env_str.split_once('=') {
                vars.push((key.to_string(), Some(val.to_string())));
            }
            i += 1;
        }
    }

    mpk_debug(format!("parsed {} args, {} env vars", args.len(), vars.len()));

    // Step 2: Tear down the existing namespace context now that arguments are safely copied.
    let cage = get_cage(cage_id)
        .ok_or_else(|| anyhow::anyhow!("cage {} not found", cage_id))?;
    {
        let runtime_info = cage.runtime_info.read();
        if let Some(mpk_info) = runtime_info.as_any().downcast_ref::<MPKRuntimeInfo>() {
            mpk_debug("tearing down old namespace context");
            unsafe {
                if !mpk_info.loader_libc_handle.is_null() {
                    libc::dlclose(mpk_info.loader_libc_handle);
                }
                if !mpk_info.loader_cage_handle.is_null() {
                    libc::dlclose(mpk_info.loader_cage_handle);
                }
            }
            // Unmap the old cage memory only when running in the same process
            // (cage_pid == 0).  For forked children (cage_pid != 0) the memory
            // lives in the child's address space and is reclaimed when we kill it.
            if mpk_info.pid == 0 && !mpk_info.memory_base.is_null() && mpk_info.memory_size > 0 {
                mpk_debug("unmapping old cage memory region");
                unsafe { libc::munmap(mpk_info.memory_base, mpk_info.memory_size); }
            }
            mpk_debug("old namespace context torn down");
            
            let my_pid = unsafe { libc::getpid() };
            let cage_pid = mpk_info.pid;
            // If the cage has a non-zero PID and it's different from our own,
            // kill that process (it's a forked child)
            if cage_pid != 0 {
                assert!(
                    cage_pid != my_pid,
                    "mpk_exit: Cannot kill self (cage_pid={}, my_pid={})",
                    cage_pid, my_pid
                );
                
                mpk_debug(format!("mpk_exit: killing child process {}", cage_pid));
                unsafe {
                    libc::kill(cage_pid, libc::SIGKILL);
                }
            }
        }
        else {
            bail!("cage {} does not have MPKRuntimeInfo; cannot exec", cage_id);
        }
    }

    // Step 5: Load the .so in a fresh dlmopen namespace
    mpk_debug("calling dlmopen for new guest .so");
    let handle = unsafe { libc::dlmopen(libc::LM_ID_NEWLM, c_so_path.as_ptr(), libc::RTLD_NOW) };
    if handle.is_null() {
        let err_msg = unsafe {
            let p = libc::dlerror();
            if p.is_null() {
                "<unknown dlerror>"
            } else {
                CStr::from_ptr(p).to_str().unwrap_or("<utf8 error>")
            }
        };
        bail!("dlmopen failed for {}: {}", so_path, err_msg);
    }
    mpk_debug(format!("dlmopen succeeded: handle={handle:p}"));

    // Retrieve the namespace id
    let mut lmid: libc::Lmid_t = 0;
    unsafe {
        libc::dlinfo(handle, RTLD_DI_LMID, &mut lmid as *mut _ as *mut c_void);
    }
    mpk_debug(format!("namespace id resolved: lmid={lmid}"));

    // Step 6: Walk the link_map chain to find the custom libc
    let mut lm: *mut LinkMap = std::ptr::null_mut();
    if unsafe { libc::dlinfo(handle, RTLD_DI_LINKMAP, &mut lm as *mut _ as *mut c_void) } != 0 {
        unsafe { libc::dlclose(handle) };
        bail!("dlinfo RTLD_DI_LINKMAP failed");
    }

    let mut libc_name_ptr: *const c_char = std::ptr::null();
    let mut current: *mut LinkMap = lm;
    while !current.is_null() {
        let name_ptr = unsafe { (*current).l_name };
        if !name_ptr.is_null() {
            let name = unsafe { CStr::from_ptr(name_ptr) }.to_str().unwrap_or("");
            if name.contains("libc.so") {
                libc_name_ptr = name_ptr;
                mpk_debug(format!("found custom libc: {name}"));
                break;
            }
        }
        current = unsafe { (*current).l_next };
    }

    if libc_name_ptr.is_null() {
        unsafe { libc::dlclose(handle) };
        bail!("could not find custom libc in dlmopen namespace for {}", so_path);
    }

    // Step 7: Obtain handle to the custom libc
    let libc_handle = unsafe {
        libc::dlmopen(lmid, libc_name_ptr, libc::RTLD_NOW | libc::RTLD_NOLOAD)
    };
    if libc_handle.is_null() {
        unsafe { libc::dlclose(handle) };
        bail!("failed to obtain handle to custom libc");
    }
    mpk_debug(format!("custom libc handle acquired: {libc_handle:p}"));

    // Step 8: Register syscall interposition handler
    let sym_name = CString::new("__enable_syscall_interpose").unwrap();
    let sym_ptr = unsafe { libc::dlsym(libc_handle, sym_name.as_ptr()) };
    if sym_ptr.is_null() {
        let err = unsafe {
            let p = libc::dlerror();
            if p.is_null() { "<unknown>" } else { CStr::from_ptr(p).to_str().unwrap_or("<utf8>") }
        };
        unsafe {
            libc::dlclose(libc_handle);
            libc::dlclose(handle);
        }
        bail!("__enable_syscall_interpose not found in custom libc: {}", err);
    }

    let enable_interpose: EnableInterposeF = unsafe { std::mem::transmute(sym_ptr) };
    ENABLE_INTERPOSE_PTR.store(sym_ptr as u64, Ordering::Release);
    let ret = unsafe { enable_interpose(Some(lind_syscall_handler)) };
    if ret != 0 {
        unsafe {
            libc::dlclose(libc_handle);
            libc::dlclose(handle);
        }
        bail!("__enable_syscall_interpose returned {}", ret);
    }
    mpk_debug("syscall interposition handler registered");
    
    // Step 9: Map fresh 4 GB for the new program, initialize vmmap, and update RuntimeInfo.
    mpk_debug("mapping 4 GB cage memory for new program with MAP_NORESERVE");
    let memory_base = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            MPK_MEMORY_SIZE,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_NORESERVE,
            -1,
            0,
        )
    };
    if memory_base == libc::MAP_FAILED {
        unsafe {
            libc::dlclose(libc_handle);
            libc::dlclose(handle);
        }
        bail!("mmap for new cage memory failed: {}", std::io::Error::last_os_error());
    }
    mpk_debug(format!("new cage memory mapped at {memory_base:p}"));
    cage::init_vmmap(cage_id, memory_base as usize, None, VmmapBitWidth::Vmmap64Bit);
    let mpk_info = MPKRuntimeInfo::new(handle, libc_handle, enable_interpose, 0, memory_base, MPK_MEMORY_SIZE);
    *cage.runtime_info.write() = Box::new(mpk_info);
    mpk_debug(format!("updated MPKRuntimeInfo for cage {}", cage_id));


    // Step 10: Build argc/argv/envp and call main
    let c_args: Vec<CString> = args.iter()
        .map(|s| CString::new(s.as_str()).unwrap())
        .collect();
    let mut argv: Vec<*const c_char> = c_args.iter().map(|s| s.as_ptr()).collect();
    argv.push(std::ptr::null());

    let c_envs: Vec<CString> = vars.iter()
        .map(|(k, v)| {
            let val = v.as_deref().unwrap_or("");
            CString::new(format!("{}={}", k, val)).unwrap()
        })
        .collect();
    let mut envp: Vec<*const c_char> = c_envs.iter().map(|s| s.as_ptr()).collect();
    envp.push(std::ptr::null());

    let main_sym = CString::new("main").unwrap();
    let main_ptr = unsafe { libc::dlsym(handle, main_sym.as_ptr()) };
    if main_ptr.is_null() {
        unsafe {
            libc::dlclose(libc_handle);
            libc::dlclose(handle);
        }
        bail!("could not find 'main' symbol in {}", so_path);
    }
    mpk_debug(format!("resolved main at {main_ptr:p}"));

    type MainFn = unsafe extern "C" fn(c_int, *const *const c_char, *const *const c_char) -> c_int;
    let main_fn: MainFn = unsafe { std::mem::transmute(main_ptr) };
    let argc = (argv.len() - 1) as c_int;
    
    mpk_debug(format!("calling main with argc={argc}"));
    let exit_code = unsafe { main_fn(argc, argv.as_ptr(), envp.as_ptr()) };
    mpk_debug(format!("main returned exit_code={exit_code}"));

    //invoke exit syscall to terminate the process
    let _ = threei::make_syscall(
        cage_id,
        EXIT_SYSCALL as u64,
        0, // _syscall_name: unused for native
        cage_id,
        exit_code as u64,
        cage_id,
        0, UNUSED_ID, 0, UNUSED_ID, 0, UNUSED_ID, 0, UNUSED_ID, 0, UNUSED_ID
    );

    Ok(exit_code as i32)
}

pub fn execute_mpk(lindboot_cli: CliOptions, cage_id: u64) -> anyhow::Result<i32> {
    let so_path = lindboot_cli.wasm_file();
    
    mpk_debug(format!("starting execute_mpk for {}, cwd={}", so_path, std::env::current_dir().unwrap().display()));
    
    //step 0: locate the .so file, turn it into fully qualified path for dlmopen
    let canonical_so_path = std::fs::canonicalize(&so_path)
        .context("failed to canonicalize .so path")?;
    let so_path = canonical_so_path
        .to_str()
        .context("invalid UTF-8 in .so path")?
        .to_owned();

    let c_so_path = CString::new(so_path.as_str()).context("NUL byte in .so path")?;


    // Step 1: Load the .so in a fresh dlmopen namespace so its custom glibc
    //         is completely isolated from the host libc.
    mpk_debug("calling dlmopen for guest .so");
    let handle =
        unsafe { libc::dlmopen(libc::LM_ID_NEWLM, c_so_path.as_ptr(), libc::RTLD_NOW) };
    if handle.is_null() {
        let err_msg = unsafe {
            let p = libc::dlerror();
            if p.is_null() {
                "<unknown dlerror>"
            } else {
                CStr::from_ptr(p).to_str().unwrap_or("<utf8 error>")
            }
        };
        bail!("dlmopen failed for {}: {}", so_path, err_msg);
    }
    mpk_debug(format!("dlmopen succeeded: handle={handle:p}"));

    // Retrieve the namespace id assigned to this new namespace.
    let mut lmid: libc::Lmid_t = 0;
    mpk_debug("querying RTLD_DI_LMID");
    unsafe {
        libc::dlinfo(
            handle,
            RTLD_DI_LMID,
            &mut lmid as *mut _ as *mut c_void,
        );
    }
    mpk_debug(format!("namespace id resolved: lmid={lmid}"));

    // Step 2: Walk the link_map chain to find the custom libc loaded in the
    //         new namespace alongside our .so.
    let mut lm: *mut LinkMap = std::ptr::null_mut();
    mpk_debug("querying RTLD_DI_LINKMAP");
    if unsafe {
        libc::dlinfo(
            handle,
            RTLD_DI_LINKMAP,
            &mut lm as *mut _ as *mut c_void,
        )
    } != 0
    {
        unsafe { libc::dlclose(handle) };
        bail!("dlinfo RTLD_DI_LINKMAP failed");
    }

    mpk_debug(format!("walking link_map chain starting at {lm:p}"));

    let mut libc_name_ptr: *const c_char = std::ptr::null();
    let mut current: *mut LinkMap = lm;
    while !current.is_null() {
        let name_ptr = unsafe { (*current).l_name };
        if !name_ptr.is_null() {
            let name = unsafe { CStr::from_ptr(name_ptr) }.to_str().unwrap_or("");
            mpk_debug(format!("link_map entry: {name}"));
            if name.contains("libc.so") {
                libc_name_ptr = name_ptr;
                mpk_debug(format!("selected custom libc: {name}"));
                break;
            }
        }
        current = unsafe { (*current).l_next };
    }

    if libc_name_ptr.is_null() {
        unsafe { libc::dlclose(handle) };
        bail!(
            "could not find custom libc in dlmopen namespace for {}",
            so_path
        );
    }

    // Step 3: Obtain a handle to the custom libc (already mapped;
    //         RTLD_NOLOAD prevents a second load) so we can resolve its
    //         private symbols.
    mpk_debug("opening custom libc with RTLD_NOLOAD");
    let libc_handle = unsafe {
        libc::dlmopen(
            lmid,
            libc_name_ptr,
            libc::RTLD_NOW | libc::RTLD_NOLOAD,
        )
    };
    if libc_handle.is_null() {
        unsafe { libc::dlclose(handle) };
        bail!("failed to obtain handle to custom libc");
    }
    mpk_debug(format!("custom libc handle acquired: {libc_handle:p}"));

    // Step 4: Register lind_syscall_handler as the interposition hook.
    //         After this point every syscall issued from inside the new
    //         namespace goes through 3i → RawPOSIX instead of the kernel.
    let sym_name = CString::new("__enable_syscall_interpose").unwrap();
    mpk_debug("resolving __enable_syscall_interpose");
    let sym_ptr = unsafe { libc::dlsym(libc_handle, sym_name.as_ptr()) };
    if sym_ptr.is_null() {
        let err = unsafe {
            let p = libc::dlerror();
            if p.is_null() {
                "<unknown>"
            } else {
                CStr::from_ptr(p).to_str().unwrap_or("<utf8>")
            }
        };
        unsafe {
            libc::dlclose(libc_handle);
            libc::dlclose(handle);
        }
        mpk_debug(format!("resolved __enable_syscall_interpose at {sym_ptr:p}"));
        bail!(
            "__enable_syscall_interpose not found in custom libc: {}",
            err
        );
    }

    mpk_debug("registering syscall interposition handler");

    let enable_interpose: EnableInterposeF = unsafe { std::mem::transmute(sym_ptr) };
    // Publish the resolved function pointer so that mpk_clone_syscall_entry can
    // re-register a new handler inside the child process after fork.
    ENABLE_INTERPOSE_PTR.store(sym_ptr as u64, Ordering::Release);
    let ret = unsafe { enable_interpose(Some(lind_syscall_handler)) };
    if ret != 0 {
        unsafe {
            libc::dlclose(libc_handle);
            libc::dlclose(handle);
    mpk_debug("syscall interposition handler registered successfully");
        }
        bail!("__enable_syscall_interpose returned {}", ret);
    mpk_debug(format!(
        "building argv/envp: args={}, vars={}",
        lindboot_cli.args.len(),
        lindboot_cli.vars.len()
    ));
    }

    //step 4.1: debug print the resolved addresses of fork, clone, __clone_internal
    let fork_sym = CString::new("fork").unwrap();
    let fork_ptr = unsafe { libc::dlsym(libc_handle, fork_sym.as_ptr()) };
    mpk_debug(format!("resolved fork at {fork_ptr:p}"));

    let clone_sym = CString::new("clone").unwrap();
    let clone_ptr = unsafe { libc::dlsym(libc_handle, clone_sym.as_ptr()) };
    mpk_debug(format!("resolved clone at {clone_ptr:p}"));

    let clone_internal_sym = CString::new("__clone_internal").unwrap();
    let clone_internal_ptr = unsafe { libc::dlsym(libc_handle, clone_internal_sym.as_ptr()) };
    mpk_debug(format!("resolved __clone_internal at {clone_internal_ptr:p}"));

    // Step 4.2: Map 4 GB for the cage's virtual address space with MAP_NORESERVE,
    // then set up MPKRuntimeInfo and store it in the cage.
    mpk_debug("mapping 4 GB cage memory with MAP_NORESERVE");
    let memory_base = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            MPK_MEMORY_SIZE,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_NORESERVE,
            -1,
            0,
        )
    };
    if memory_base == libc::MAP_FAILED {
        unsafe {
            libc::dlclose(libc_handle);
            libc::dlclose(handle);
        }
        bail!("mmap failed: {}", std::io::Error::last_os_error());
    }
    mpk_debug(format!("cage memory mapped at {memory_base:p}"));

    // Get the cage, initialize its vmmap, and store MPKRuntimeInfo.
    let cage = get_cage(cage_id)
        .ok_or_else(|| anyhow::anyhow!("cage {} not found", cage_id))?;
    cage::init_vmmap(cage_id, memory_base as usize, None, VmmapBitWidth::Vmmap64Bit);
    let mpk_info = MPKRuntimeInfo::new(handle, libc_handle, enable_interpose, 0, memory_base, MPK_MEMORY_SIZE);
    *cage.runtime_info.write() = Box::new(mpk_info);
    mpk_debug(format!("MPKRuntimeInfo stored in cage {}", cage_id));

    //Step 5: Notify threei of the cage runtime type
    // (syscall handler registration is now done once at boot by shims::register_syscall_entries)
    threei::set_cage_runtime(cage_id, threei_const::RUNTIME_TYPE_MPK);


    // Step 6: Build argc / argv / envp from CliOptions.
    let c_args: Vec<CString> = lindboot_cli
        .args
        .iter()
        .map(|s| CString::new(s.as_str()).unwrap())
        .collect();
    let c_envs: Vec<CString> = lindboot_cli
        .vars
        .iter()
        .map(|(k, v)| {
            let val = v.as_deref().unwrap_or("");
            CString::new(format!("{}={}", k, val)).unwrap()
        })
        .collect();

    let main_sym = CString::new("main").unwrap();
    mpk_debug("resolving main");
    let main_ptr = unsafe { libc::dlsym(handle, main_sym.as_ptr()) };
    if main_ptr.is_null() {
        unsafe {
            libc::dlclose(libc_handle);
            libc::dlclose(handle);
        }
        bail!("could not find 'main' symbol in {}", so_path);
    }
    mpk_debug(format!("resolved main at {main_ptr:p}"));

    type MainFn =
        unsafe extern "C" fn(c_int, *const *const c_char, *const *const c_char) -> c_int;
    let main_fn: MainFn = unsafe { std::mem::transmute(main_ptr) };
    let argc = c_args.len() as c_int;

    mpk_debug(format!("spawning main thread with argc={argc}"));

    // Step 7: Call main inside a dedicated thread.
    // c_args and c_envs are moved into the closure so the CString backing
    // data outlives the raw pointer slices built from them inside the thread.
    // main_fn and argc are Copy, so they are simply captured by value.
    // let join_handle = std::thread::spawn(move || {
    //     let mut argv: Vec<*const c_char> = c_args.iter().map(|s| s.as_ptr()).collect();
    //     argv.push(std::ptr::null());
    //     let mut envp: Vec<*const c_char> = c_envs.iter().map(|s| s.as_ptr()).collect();
    //     envp.push(std::ptr::null());
    //     unsafe { main_fn(argc, argv.as_ptr(), envp.as_ptr()) }
    // });

    let mut argv: Vec<*const c_char> = c_args.iter().map(|s| s.as_ptr()).collect();
    argv.push(std::ptr::null());
    let mut envp: Vec<*const c_char> = c_envs.iter().map(|s| s.as_ptr()).collect();
    envp.push(std::ptr::null());
    let exit_code = unsafe { main_fn(argc, argv.as_ptr(), envp.as_ptr()) }; 

    // mpk_debug("waiting for main thread to finish");
    // let exit_code = join_handle.join().unwrap_or(-1);
    // mpk_debug(format!("main returned exit_code={exit_code}"));



    mpk_debug("execute_mpk completed successfully");

    Ok(exit_code as i32)
}