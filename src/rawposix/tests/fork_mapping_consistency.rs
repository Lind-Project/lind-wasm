use cage::memory::vmmap::{MemoryBackingType, Vmmap, VmmapOps};
use cage::timer::IntervalTimer;
use cage::{Cage, Condvar, DashMap, Mutex, RwLock};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;
use sysdefs::constants::lind_platform_const::{UNUSED_ARG, UNUSED_ID};

const CAGE_ID: u64 = 81;
const TEST_RUNTIME: u64 = 0x7e57;
const PAGE: usize = 4096;

fn test_cage(base: usize) -> Cage {
    let mut vmmap = Vmmap::new();
    vmmap.set_base_address(base);
    vmmap
        .add_entry_with_overwrite(
            1,
            1,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            MemoryBackingType::Anonymous,
            0,
            PAGE as i64,
            CAGE_ID,
        )
        .unwrap();
    Cage {
        cageid: CAGE_ID,
        parent: CAGE_ID,
        cwd: RwLock::new(Arc::new(PathBuf::from("/"))),
        rev_shm: Mutex::new(Vec::new()),
        main_threadid: RwLock::new(0),
        interval_timer: IntervalTimer::new(CAGE_ID),
        epoch_handler: DashMap::new(),
        os_tid_map: DashMap::new(),
        pending_signals: RwLock::new(Vec::new()),
        signalhandler: DashMap::new(),
        sigset: AtomicU64::new(0),
        zombies: Mutex::new(Vec::new()),
        wait_cond: Condvar::new(),
        child_num: AtomicU64::new(0),
        vmmap: RwLock::new(vmmap),
        final_exit_status: RwLock::new(None),
        exit_group_initiated: AtomicBool::new(false),
        is_dead: AtomicBool::new(false),
        grate_inflight: AtomicU64::new(0),
    }
}

fn read_byte(address: usize) -> Option<u8> {
    let mut value = 0u8;
    let local = libc::iovec {
        iov_base: (&mut value as *mut u8).cast(),
        iov_len: 1,
    };
    let remote = libc::iovec {
        iov_base: address as *mut libc::c_void,
        iov_len: 1,
    };
    let count = unsafe { libc::process_vm_readv(libc::getpid(), &local, 1, &remote, 1, 0) };
    (count == 1).then_some(value)
}

#[test]
fn host_mapping_changes_wait_for_fork_snapshot() {
    unsafe {
        let base = libc::mmap(
            std::ptr::null_mut(),
            2 * PAGE,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        );
        assert_ne!(base, libc::MAP_FAILED);
        let address = base as usize + PAGE;
        *(address as *mut u8) = 0x5a;
        cage::add_cage(CAGE_ID, test_cage(base as usize));
        let runtime = Arc::new(lind_runtime::testing::RecordingRuntime::new(TEST_RUNTIME));
        lind_runtime::register_runtime(runtime.clone());
        threei::set_cage_runtime(CAGE_ID, TEST_RUNTIME);
        let cage = cage::get_cage(CAGE_ID).unwrap();

        let snapshot = cage.vmmap.read();
        let (started_tx, started_rx) = mpsc::channel();
        let mmap_worker = thread::spawn(move || {
            started_tx.send(()).unwrap();
            rawposix::fs_calls::mmap_syscall(
                CAGE_ID,
                PAGE as u64,
                CAGE_ID,
                PAGE as u64,
                CAGE_ID,
                (libc::PROT_READ | libc::PROT_WRITE) as u64,
                CAGE_ID,
                (libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_FIXED) as u64,
                CAGE_ID,
                -1i64 as u64,
                CAGE_ID,
                0,
                CAGE_ID,
            )
        });
        started_rx.recv().unwrap();
        thread::sleep(Duration::from_millis(100));
        assert!(!mmap_worker.is_finished());
        assert_eq!(read_byte(address), Some(0x5a));
        drop(snapshot);
        assert_eq!(mmap_worker.join().unwrap(), PAGE as i64);
        assert_eq!(read_byte(address), Some(0));
        *(address as *mut u8) = 0x5a;

        let snapshot = cage.vmmap.read();
        let (started_tx, started_rx) = mpsc::channel();
        let protect_worker = thread::spawn(move || {
            started_tx.send(()).unwrap();
            rawposix::fs_calls::mprotect_syscall(
                CAGE_ID,
                address as u64,
                CAGE_ID,
                PAGE as u64,
                CAGE_ID,
                libc::PROT_NONE as u64,
                CAGE_ID,
                UNUSED_ARG,
                UNUSED_ID,
                UNUSED_ARG,
                UNUSED_ID,
                UNUSED_ARG,
                UNUSED_ID,
            )
        });
        started_rx.recv().unwrap();
        thread::sleep(Duration::from_millis(100));
        assert!(!protect_worker.is_finished());
        assert_eq!(read_byte(address), Some(0x5a));
        drop(snapshot);
        assert_eq!(protect_worker.join().unwrap(), 0);
        assert_eq!(read_byte(address), None);

        assert_eq!(
            runtime.calls(),
            [
                format!("do_mmap({CAGE_ID}, false)"),
                format!("do_mprotect({CAGE_ID})")
            ]
        );
        threei::remove_cage_runtime(CAGE_ID);
        cage::remove_cage(CAGE_ID);
        assert_eq!(libc::munmap(base, 2 * PAGE), 0);
    }
}
