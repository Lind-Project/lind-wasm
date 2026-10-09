use cage::memory::vmmap::Vmmap;
use cage::timer::IntervalTimer;
use cage::{add_cage, cagetable_init, get_cage, remove_cage, Cage, NullRuntimeInfo};
use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use rawposix::sys_calls::prlimit64_syscall;
use std::mem::{offset_of, size_of};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use sysdefs::constants::lind_platform_const::{
    MAX_CAGEID, MAX_LINEAR_MEMORY_SIZE, RUNTIME_TYPE_MPK, RUNTIME_TYPE_WASMTIME, UNUSED_ARG,
    UNUSED_ID,
};
use sysdefs::constants::sys_const::{
    RLIMIT_AS, RLIMIT_CORE, RLIMIT_DATA, RLIMIT_NOFILE, RLIMIT_NPROC, RLIMIT_RSS, RLIMIT_STACK,
};
use sysdefs::constants::Errno;
use sysdefs::data::fs_struct::{Rlimit, Rlimit64};
use typemap::datatype_conversion::convert_rlimit_to_user;

fn get_limit(resource: u32, buffer: u64) -> i64 {
    prlimit64_syscall(
        1,
        0,
        1,
        resource as u64,
        1,
        0,
        1,
        buffer,
        2,
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
    )
}

#[test]
fn prlimit64_uses_calling_cage_runtime_layout() {
    assert_eq!(size_of::<Rlimit>(), 8);
    assert_eq!(offset_of!(Rlimit, rlim_max), 4);
    assert_eq!(size_of::<Rlimit64>(), 16);
    assert_eq!(offset_of!(Rlimit64, rlim_max), 8);
    assert_eq!(size_of::<Rlimit64>(), size_of::<libc::rlimit>());
    assert_eq!(
        offset_of!(Rlimit64, rlim_max),
        offset_of!(libc::rlimit, rlim_max)
    );

    cagetable_init();
    add_cage(
        1,
        Cage {
            cageid: 1,
            parent: 1,
            cwd: RwLock::new(Arc::new(PathBuf::from("/"))),
            rev_shm: Mutex::new(Vec::new()),
            signalhandler: DashMap::new(),
            sigset: AtomicU64::new(0),
            pending_signals: RwLock::new(vec![]),
            epoch_handler: DashMap::new(),
            os_tid_map: DashMap::new(),
            main_threadid: RwLock::new(0),
            interval_timer: IntervalTimer::new(1),
            zombies: RwLock::new(vec![]),
            child_num: AtomicU64::new(0),
            vmmap: RwLock::new(Vmmap::new()),
            final_exit_status: RwLock::new(None),
            exit_group_initiated: AtomicBool::new(false),
            is_dead: AtomicBool::new(false),
            grate_inflight: AtomicU64::new(0),
            runtime_info: RwLock::new(Box::new(NullRuntimeInfo)),
            runtime_type: AtomicU64::new(RUNTIME_TYPE_WASMTIME),
        },
    );
    let cage = get_cage(1).unwrap();
    let resources = [
        (RLIMIT_NOFILE, 1024),
        (RLIMIT_STACK, 8 * 1024 * 1024),
        (RLIMIT_AS, MAX_LINEAR_MEMORY_SIZE),
        (RLIMIT_DATA, MAX_LINEAR_MEMORY_SIZE),
        (RLIMIT_RSS, MAX_LINEAR_MEMORY_SIZE),
        (RLIMIT_NPROC, MAX_CAGEID as u64),
        (RLIMIT_CORE, 0),
    ];

    for (resource, expected) in resources {
        let mut wasm_buffer = [0xA5A5_A5A5u32; 4];
        let wasm_address = wasm_buffer[1..].as_mut_ptr() as u64;
        assert_eq!(get_limit(resource, wasm_address), 0);
        assert_eq!(
            wasm_buffer,
            [0xA5A5_A5A5, expected as u32, expected as u32, 0xA5A5_A5A5]
        );

        cage.runtime_type.store(RUNTIME_TYPE_MPK, Ordering::Release);
        let mut native_buffer = [0xA5A5_A5A5_A5A5_A5A5u64; 4];
        let native_address = native_buffer[1..].as_mut_ptr() as u64;
        assert_eq!(get_limit(resource, native_address), 0);
        assert_eq!(
            native_buffer,
            [
                0xA5A5_A5A5_A5A5_A5A5,
                expected,
                expected,
                0xA5A5_A5A5_A5A5_A5A5
            ]
        );
        cage.runtime_type
            .store(RUNTIME_TYPE_WASMTIME, Ordering::Release);
    }

    for runtime in [RUNTIME_TYPE_WASMTIME, RUNTIME_TYPE_MPK] {
        cage.runtime_type.store(runtime, Ordering::Release);
        assert_eq!(get_limit(RLIMIT_NOFILE, 0), 0);
    }

    let mut native_buffer = [0u64; 2];
    let native_address = native_buffer.as_mut_ptr() as u64;
    convert_rlimit_to_user(
        1,
        native_address,
        2,
        Rlimit64 {
            rlim_cur: u32::MAX as u64 + 1,
            rlim_max: u64::MAX,
        },
    )
    .unwrap();
    assert_eq!(native_buffer, [u32::MAX as u64 + 1, u64::MAX]);

    cage.runtime_type
        .store(RUNTIME_TYPE_WASMTIME, Ordering::Release);
    let mut wasm_buffer = [0xA5A5_A5A5u32; 2];
    let wasm_address = wasm_buffer.as_mut_ptr() as u64;
    for (rlim_cur, rlim_max) in [(u64::MAX, 1024), (1024, u64::MAX)] {
        assert!(matches!(
            convert_rlimit_to_user(1, wasm_address, 2, Rlimit64 { rlim_cur, rlim_max },),
            Err(Errno::EOVERFLOW)
        ));
        assert_eq!(wasm_buffer, [0xA5A5_A5A5; 2]);
    }
    remove_cage(1);
}
