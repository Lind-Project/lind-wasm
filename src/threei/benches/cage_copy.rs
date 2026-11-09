//! Benchmarks for the cross-cage address checks and `copy_data_between_cages`
//! in Strncpy mode.
//!
//! Real `Cage`s are registered in the global cage table and backed by a
//! page-aligned host buffer that stands in for wasm linear memory. Each cage's
//! vmmap is populated with several entries of mixed protection so lookups go
//! through the interval map the same way they do at runtime.
//!
//! Only public APIs are used (`cage::memory::check_addr_read`,
//! `threei::copy_data_between_cages`) so the same file can be run against
//! different implementations of those functions.
use cage::memory::{check_addr_read, init_vmmap, MemoryBackingType, VmmapOps};
use cage::timer::IntervalTimer;
use cage::{
    add_cage, cagetable_init, AtomicBool, AtomicU64, Cage, Condvar, Mutex, PathBuf, RwLock,
};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::alloc::{alloc_zeroed, Layout};
use std::sync::{Arc, Barrier, OnceLock};
use std::time::{Duration, Instant};
use sysdefs::constants::fs_const::{MAP_ANONYMOUS, MAP_PRIVATE, PAGESIZE, PROT_READ, PROT_WRITE};

const SRC_CAGE: u64 = 1;
const DST_CAGE: u64 = 2;
const MEM_PAGES: u32 = 1024; // 4 MiB per cage
const STRNCPY: u64 = 1;
// Max length handed to Strncpy, like a PATH_MAX-style bound but large enough
// for the longest string below.
const MAX_LEN: u64 = 1 << 20;
// Strings start at this offset inside their first page.
const STR_PAGE_OFFSET: u64 = 0x100;
// Data region (RW) where strings and destination buffers live.
const DATA_FIRST_PAGE: u32 = 256;

static BASES: OnceLock<(u64, u64)> = OnceLock::new();

fn make_cage(cageid: u64) -> Cage {
    Cage {
        cageid,
        parent: 1,
        cwd: RwLock::new(Arc::new(PathBuf::from("/"))),
        rev_shm: Mutex::new(Vec::new()),
        signalhandler: Default::default(),
        sigset: AtomicU64::new(0),
        pending_signals: RwLock::new(vec![]),
        epoch_handler: Default::default(),
        os_tid_map: Default::default(),
        main_threadid: RwLock::new(0),
        interval_timer: IntervalTimer::new(cageid),
        zombies: Mutex::new(vec![]),
        wait_cond: Condvar::new(),
        child_num: AtomicU64::new(0),
        vmmap: RwLock::new(cage::Vmmap::new()),
        final_exit_status: RwLock::new(None),
        exit_group_initiated: AtomicBool::new(false),
        is_dead: AtomicBool::new(false),
        grate_inflight: AtomicU64::new(0),
    }
}

/// Register a cage backed by a fresh page-aligned buffer and a vmmap that
/// roughly mirrors a running program: a read-only low region, the RW data/heap
/// region, a few mmap'd chunks with mixed protection, and a stack at the top.
fn setup_cage(cageid: u64) -> u64 {
    add_cage(cageid, make_cage(cageid));

    let size = (MEM_PAGES as usize) * PAGESIZE as usize;
    let layout = Layout::from_size_align(size, 1 << 16).unwrap();
    let base = unsafe { alloc_zeroed(layout) } as usize;
    assert!(base != 0);
    init_vmmap(cageid, base, None);

    let cage = cage::get_cage(cageid).unwrap();
    let mut vmmap = cage.vmmap.write();
    let flags = (MAP_PRIVATE | MAP_ANONYMOUS) as i32;
    let rw = PROT_READ | PROT_WRITE;
    // (first page, npages, prot)
    let layout_entries: &[(u32, u32, i32)] = &[
        (0, 64, PROT_READ),
        (64, 192, rw),
        (DATA_FIRST_PAGE, 512, rw),
        (768, 32, PROT_READ),
        (800, 64, rw),
        (864, 32, PROT_READ),
        (896, 64, rw),
        (992, 32, rw),
    ];
    for &(page, npages, prot) in layout_entries {
        vmmap
            .add_entry_with_overwrite(
                page,
                npages,
                prot,
                rw,
                flags,
                MemoryBackingType::Anonymous,
                0,
                0,
                cageid,
            )
            .unwrap();
    }
    base as u64
}

/// Returns the host base addresses of the (source, destination) cages.
fn init() -> (u64, u64) {
    *BASES.get_or_init(|| {
        cagetable_init();
        (setup_cage(SRC_CAGE), setup_cage(DST_CAGE))
    })
}

fn data_addr(base: u64, page: u32) -> u64 {
    base + ((DATA_FIRST_PAGE + page) as u64) * PAGESIZE as u64
}

/// Writes a string of `len` non-NUL bytes followed by '\0' into the source
/// cage and returns its address.
fn place_string(src_base: u64, len: usize) -> u64 {
    let addr = data_addr(src_base, 0) + STR_PAGE_OFFSET;
    unsafe {
        let p = addr as *mut u8;
        for i in 0..len {
            *p.add(i) = b'a' + (i % 26) as u8;
        }
        *p.add(len) = 0;
    }
    addr
}

fn strncpy(src: u64, dst: u64) -> u64 {
    threei::copy_data_between_cages(
        SRC_CAGE, SRC_CAGE, src, SRC_CAGE, dst, DST_CAGE, MAX_LEN, 0, STRNCPY, 0, 0, 0, 0, 0,
    )
}

/// Runs `op` `iters` times on each of `nthreads` threads, all released together,
/// and returns the wall time. Reported time per iteration is therefore the time
/// for every thread to finish one call.
fn run_concurrent<F: Fn(usize) + Sync>(nthreads: usize, iters: u64, op: F) -> Duration {
    let barrier = Barrier::new(nthreads + 1);
    let start = std::thread::scope(|s| {
        for t in 0..nthreads {
            let barrier = &barrier;
            let op = &op;
            s.spawn(move || {
                barrier.wait();
                for _ in 0..iters {
                    op(t);
                }
            });
        }
        barrier.wait();
        Instant::now()
    });
    start.elapsed()
}

fn bench_check_addr(c: &mut Criterion) {
    let (src_base, _) = init();
    let addr = data_addr(src_base, 3) + 64;

    let mut g = c.benchmark_group("check_addr_read");
    g.bench_function("single_thread", |b| {
        b.iter(|| check_addr_read(black_box(SRC_CAGE), black_box(addr), black_box(64)))
    });
    for &n in &[2usize, 4, 8, 16] {
        g.bench_with_input(BenchmarkId::new("threads", n), &n, |b, &n| {
            b.iter_custom(|iters| {
                run_concurrent(n, iters, |_| {
                    let _ = black_box(check_addr_read(SRC_CAGE, black_box(addr), 64));
                })
            })
        });
    }
    g.finish();
}

fn bench_strncpy(c: &mut Criterion) {
    let (src_base, dst_base) = init();

    let mut g = c.benchmark_group("strncpy");
    for &len in &[16usize, 64, 256, 1024, 4096, 16384] {
        let src = place_string(src_base, len);
        let dst = data_addr(dst_base, 0) + STR_PAGE_OFFSET;
        assert_eq!(strncpy(src, dst), dst);
        g.throughput(Throughput::Bytes(len as u64 + 1));
        g.bench_with_input(BenchmarkId::from_parameter(len), &len, |b, _| {
            b.iter(|| strncpy(black_box(src), black_box(dst)))
        });
    }
    g.finish();

    // Concurrent path-sized copies from one cage, as when several threads of
    // a program make path syscalls that a grate forwards.
    let len = 64;
    let src = place_string(src_base, len);
    let mut g = c.benchmark_group("strncpy_64B");
    for &n in &[1usize, 2, 4, 8, 16] {
        g.bench_with_input(BenchmarkId::new("threads", n), &n, |b, &n| {
            b.iter_custom(|iters| {
                run_concurrent(n, iters, |t| {
                    // separate destination page per thread
                    let dst = data_addr(dst_base, 16 + t as u32) + STR_PAGE_OFFSET;
                    let _ = black_box(strncpy(black_box(src), dst));
                })
            })
        });
    }
    g.finish();
}

criterion_group!(benches, bench_check_addr, bench_strncpy);
criterion_main!(benches);
