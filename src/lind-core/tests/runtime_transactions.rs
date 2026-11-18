use cage::memory::vmmap::Vmmap;
use cage::timer::IntervalTimer;
use cage::{Cage, Condvar, Mutex, RwLock};
use dashmap::DashMap;
use lind_runtime::testing::RecordingRuntime;
use lind_runtime::{LindArgVector, LindError, LindInitArgs, UserPtr};
use std::ffi::CString;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;
use sysdefs::constants::err_const::Errno;

const RUNTIME: u64 = 0x51;

fn fresh_runtime() -> Arc<RecordingRuntime> {
    let runtime = Arc::new(RecordingRuntime::new(RUNTIME));
    lind_runtime::register_runtime(runtime.clone());
    runtime
}

fn operations(runtime: &RecordingRuntime) -> Vec<String> {
    runtime
        .calls()
        .into_iter()
        .map(|call| call[..call.find('(').unwrap()].to_owned())
        .collect()
}

fn add_parent_cage(cageid: u64) {
    cage::add_cage(
        cageid,
        Cage {
            cageid,
            parent: cageid,
            cwd: RwLock::new(Arc::new(PathBuf::from("/"))),
            rev_shm: Mutex::new(Vec::new()),
            main_threadid: RwLock::new(0),
            interval_timer: IntervalTimer::new(cageid),
            epoch_handler: DashMap::new(),
            os_tid_map: DashMap::new(),
            pending_signals: RwLock::new(Vec::new()),
            signalhandler: DashMap::new(),
            sigset: AtomicU64::new(0),
            zombies: Mutex::new(Vec::new()),
            wait_cond: Condvar::new(),
            child_num: AtomicU64::new(0),
            vmmap: RwLock::new(Vmmap::new()),
            final_exit_status: RwLock::new(None),
            exit_group_initiated: AtomicBool::new(false),
            is_dead: AtomicBool::new(false),
            grate_inflight: AtomicU64::new(0),
        },
    );
    fdtables::init_empty_cage(cageid);
    threei::handler_table::register_handler_impl(cageid, 42, 17, 123);
    threei::set_cage_runtime(cageid, RUNTIME);
}

// One test so the runtimes registered under the shared id never race.
#[test]
fn runtime_failures_roll_back_core_transactions() {
    // Launch: a failed CreateThread terminates the cage and drops the runtime binding.
    let runtime = fresh_runtime();
    runtime.fail("create_thread", Errno::EAGAIN);
    let launched = lind_core::launch_cage(
        5,
        RUNTIME,
        "/prog",
        LindInitArgs {
            argv: LindArgVector::Owned(vec!["/prog".into()]),
            envp: LindArgVector::Owned(Vec::new()),
        },
    );
    assert!(matches!(launched, Err(LindError::Other(Errno::EAGAIN))));
    assert_eq!(
        runtime.calls(),
        ["init_cage(5, /prog, false)", "create_thread(0x5, 1)", "terminate_cage(0x5)"]
    );
    assert_eq!(threei::get_cage_runtime(5), None);

    // Fork: a failed DuplicateThread terminates the duplicated cage and releases the child ID.
    let runtime = fresh_runtime();
    runtime.fail("duplicate_thread", Errno::EAGAIN);
    add_parent_cage(1);
    let ret = lind_core::clone::clone_cage(1, 1, UserPtr { addr: 0, cage: 1 });
    assert_eq!(ret, -(Errno::EAGAIN as i64));
    let calls = runtime.calls();
    let child = calls[0]
        .strip_prefix("duplicate_cage(1, ")
        .and_then(|rest| rest.strip_suffix(')'))
        .unwrap()
        .parse::<u64>()
        .unwrap();
    assert_eq!(operations(&runtime), ["duplicate_cage", "duplicate_thread", "terminate_cage"]);
    assert_eq!(calls[2], format!("terminate_cage({child:#x})"));
    assert!(cage::get_cage(child).is_none());

    // Exec: a staging failure discards the staging cage and never touches the target.
    let runtime = fresh_runtime();
    runtime.fail("create_thread", Errno::ENOMEM);
    let path = CString::new("/bin/next").unwrap();
    let ret = lind_core::exec::exec(
        1,
        UserPtr { addr: path.as_ptr() as u64, cage: 1 },
        UserPtr { addr: 0, cage: 1 },
        UserPtr { addr: 0, cage: 1 },
    );
    assert_eq!(ret, -(Errno::ENOMEM as i64));
    assert_eq!(
        operations(&runtime),
        ["belongs_to_runtime", "init_cage", "create_thread", "terminate_cage"]
    );
    let calls = runtime.calls();
    let staging = calls[2].strip_prefix("create_thread(").unwrap().split(',').next().unwrap();
    assert_eq!(calls[3], format!("terminate_cage({staging})"));
}
