use cage::memory::vmmap::Vmmap;
use cage::timer::IntervalTimer;
use cage::{add_cage, cagetable_init, Cage};
use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use rawposix::fs_calls::{gettimeofday_syscall, mkdir_syscall, time_syscall, umask_syscall};
use rawposix::sys_calls::getgroups_syscall;
use std::ffi::CString;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use sysdefs::constants::err_const::VERBOSE;
use sysdefs::constants::lind_platform_const::{RUNTIME_TYPE_WASMTIME, UNUSED_ARG, UNUSED_ID};
use sysdefs::constants::Errno;

const CAGE: u64 = 1;

fn call1(
    f: extern "C" fn(u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64) -> i64,
    arg1: u64,
    arg2: u64,
) -> i64 {
    f(
        CAGE, arg1, CAGE, arg2, CAGE, UNUSED_ARG, UNUSED_ID, UNUSED_ARG, UNUSED_ID, UNUSED_ARG,
        UNUSED_ID, UNUSED_ARG, UNUSED_ID,
    )
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[test]
fn umask_is_per_cage_and_applied_to_mkdir() {
    cagetable_init();
    add_cage(
        CAGE,
        Cage {
            cageid: CAGE,
            parent: CAGE,
            cwd: RwLock::new(Arc::new(PathBuf::from("/"))),
            umask: AtomicU32::new(0o022),
            rev_shm: Mutex::new(Vec::new()),
            signalhandler: DashMap::new(),
            sigset: AtomicU64::new(0),
            pending_signals: RwLock::new(vec![]),
            epoch_handler: DashMap::new(),
            os_tid_map: DashMap::new(),
            main_threadid: RwLock::new(0),
            interval_timer: IntervalTimer::new(CAGE),
            zombies: Mutex::new(vec![]),
            wait_cond: parking_lot::Condvar::new(),
            child_num: AtomicU64::new(0),
            vmmap: RwLock::new(Vmmap::new()),
            final_exit_status: RwLock::new(None),
            exit_group_initiated: AtomicBool::new(false),
            last_thread_id: std::sync::atomic::AtomicI32::new(cage::MAIN_THREAD_ID),
            is_dead: AtomicBool::new(false),
            grate_inflight: AtomicU64::new(0),
            runtime_type: AtomicU64::new(RUNTIME_TYPE_WASMTIME),
        },
    );
    let host_umask_before = unsafe { libc::umask(0o022) };
    unsafe { libc::umask(host_umask_before) };

    assert_eq!(call1(umask_syscall, 0o077, UNUSED_ARG), 0o022);
    // Only permission bits are kept.
    assert_eq!(call1(umask_syscall, 0o7027, UNUSED_ARG), 0o077);
    assert_eq!(call1(umask_syscall, 0o077, UNUSED_ARG), 0o027);
    // The host process umask is left alone.
    let host_umask_after = unsafe { libc::umask(0o022) };
    unsafe { libc::umask(host_umask_after) };
    assert_eq!(host_umask_after, host_umask_before);

    let dir = std::env::temp_dir().join(format!("lind-umask-test-{}", std::process::id()));
    let _ = std::fs::remove_dir(&dir);
    let path = CString::new(dir.to_str().unwrap()).unwrap();
    assert_eq!(call1(mkdir_syscall, path.as_ptr() as u64, 0o777), 0);
    let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
    std::fs::remove_dir(&dir).unwrap();
    assert_eq!(mode, 0o700);
}

#[test]
fn time_returns_and_stores_current_seconds() {
    let before = now_secs();
    let mut stored: libc::time_t = 0;
    let ret = call1(time_syscall, &mut stored as *mut _ as u64, UNUSED_ARG);
    let after = now_secs();
    assert!(before <= ret && ret <= after);
    assert_eq!(stored as i64, ret);
    assert!(call1(time_syscall, 0, UNUSED_ARG) >= ret);
}

#[test]
fn gettimeofday_fills_timeval() {
    let before = now_secs();
    let mut tv = libc::timeval {
        tv_sec: 0,
        tv_usec: 0,
    };
    assert_eq!(call1(gettimeofday_syscall, &mut tv as *mut _ as u64, 0), 0);
    assert!(before <= tv.tv_sec as i64 && tv.tv_sec as i64 <= now_secs());
    assert!((0..1_000_000).contains(&tv.tv_usec));
}

#[test]
fn getgroups_reports_no_supplementary_groups() {
    let _ = VERBOSE.set(0);
    let mut list = [0xA5A5_A5A5u32; 4];
    assert_eq!(call1(getgroups_syscall, 0, 0), 0);
    assert_eq!(call1(getgroups_syscall, 4, list.as_mut_ptr() as u64), 0);
    assert_eq!(list, [0xA5A5_A5A5u32; 4]);
    assert_eq!(
        call1(getgroups_syscall, (-1i32) as u32 as u64, 0),
        -(Errno::EINVAL as i64)
    );
}
