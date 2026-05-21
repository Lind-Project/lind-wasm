//! This benchmark is intended only to compare different internal
//! implementations of the cage table under synthetic workloads.
//!
//! The `MockCage` type and `CageMap` trait below are simplified stand-ins for the
//! real cage table logic. They are used to isolate the cost of table lookup,
//! insertion, removal, locking, and concurrent access patterns.
//!
//! This benchmark does not measure full RawPOSIX/3i runtime behavior, syscall
//! dispatch overhead, or realistic cage lifecycle costs. Its purpose is only to
//! evaluate the relative performance characteristics of several possible cage
//! table implementations.
use arc_swap::ArcSwapOption;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use dashmap::DashMap;
use parking_lot::RwLock;
use std::sync::Arc;
use std::thread;

#[derive(Debug)]
struct MockCage {
    cageid: u64,
    payload: u64,
}

/// Common interface used by all benchmarked cage table implementations.
/// Each implementation provides the same add/remove/get operations so that
/// Criterion can compare their performance under identical workloads.
trait CageMap: Send + Sync + 'static {
    fn new(size: usize) -> Self
    where
        Self: Sized;

    fn add_cage(&self, cageid: usize, cage: MockCage);
    fn remove_cage(&self, cageid: usize) -> Option<Arc<MockCage>>;
    fn get_cage(&self, cageid: usize) -> Option<Arc<MockCage>>;
}

/// --------------------------------------------------------------------------------
/// Implementation 1: a single global RwLock protecting the entire table.
/// This represents the simplest design, but all reads and writes contend on
/// the same lock.
struct GlobalRwLockMap {
    inner: RwLock<Vec<Option<Arc<MockCage>>>>,
}

impl CageMap for GlobalRwLockMap {
    fn new(size: usize) -> Self {
        Self {
            inner: RwLock::new((0..size).map(|_| None).collect()),
        }
    }

    fn add_cage(&self, cageid: usize, cage: MockCage) {
        self.inner.write()[cageid] = Some(Arc::new(cage));
    }

    fn remove_cage(&self, cageid: usize) -> Option<Arc<MockCage>> {
        self.inner.write()[cageid].take()
    }

    fn get_cage(&self, cageid: usize) -> Option<Arc<MockCage>> {
        self.inner.read()[cageid].clone()
    }
}

/// --------------------------------------------------------------------------------
///
/// Implementation 2: one RwLock per table slot.
/// This reduces contention between different cage IDs, while still using
/// locking for each individual slot.
struct SlotRwLockMap {
    slots: Vec<RwLock<Option<Arc<MockCage>>>>,
}

impl CageMap for SlotRwLockMap {
    fn new(size: usize) -> Self {
        Self {
            slots: (0..size).map(|_| RwLock::new(None)).collect(),
        }
    }

    fn add_cage(&self, cageid: usize, cage: MockCage) {
        *self.slots[cageid].write() = Some(Arc::new(cage));
    }

    fn remove_cage(&self, cageid: usize) -> Option<Arc<MockCage>> {
        self.slots[cageid].write().take()
    }

    fn get_cage(&self, cageid: usize) -> Option<Arc<MockCage>> {
        self.slots[cageid].read().clone()
    }
}

/// --------------------------------------------------------------------------------
///
/// Implementation 3: DashMap-based table.
/// This uses DashMap's sharded concurrent map internally and serves as a
/// comparison point against Vec-based table designs.
struct DashMapCageMap {
    inner: DashMap<usize, Arc<MockCage>>,
}

impl CageMap for DashMapCageMap {
    fn new(_size: usize) -> Self {
        Self {
            inner: DashMap::new(),
        }
    }

    fn add_cage(&self, cageid: usize, cage: MockCage) {
        self.inner.insert(cageid, Arc::new(cage));
    }

    fn remove_cage(&self, cageid: usize) -> Option<Arc<MockCage>> {
        self.inner.remove(&cageid).map(|(_, cage)| cage)
    }

    fn get_cage(&self, cageid: usize) -> Option<Arc<MockCage>> {
        self.inner.get(&cageid).map(|entry| Arc::clone(&*entry))
    }
}

/// --------------------------------------------------------------------------------
/// Implementation 4: ArcSwap-based table.
/// This design optimizes read-mostly workloads by allowing lock-free loads
/// of Arc-backed cage entries.
struct ArcSwapCageMap {
    slots: Vec<ArcSwapOption<MockCage>>,
}

impl CageMap for ArcSwapCageMap {
    fn new(size: usize) -> Self {
        Self {
            slots: (0..size).map(|_| ArcSwapOption::from(None)).collect(),
        }
    }

    fn add_cage(&self, cageid: usize, cage: MockCage) {
        self.slots[cageid].store(Some(Arc::new(cage)));
    }

    fn remove_cage(&self, cageid: usize) -> Option<Arc<MockCage>> {
        self.slots[cageid].swap(None)
    }

    fn get_cage(&self, cageid: usize) -> Option<Arc<MockCage>> {
        self.slots[cageid].load_full()
    }
}

/// --------------------------------------------------------------------------------
///
const TABLE_SIZE: usize = 4096;
const LIVE_CAGES: usize = 1024;

fn setup_map<M: CageMap>() -> M {
    let map = M::new(TABLE_SIZE);
    for id in 0..LIVE_CAGES {
        map.add_cage(
            id,
            MockCage {
                cageid: id as u64,
                payload: id as u64,
            },
        );
    }
    map
}

// Measures the cost of repeated get_cage() operations from a single thread.
fn bench_single_thread_get<M: CageMap>(c: &mut Criterion, name: &str) {
    let map = setup_map::<M>();

    c.bench_function(name, |b| {
        let mut i = 0usize;

        b.iter(|| {
            let id = i % LIVE_CAGES;
            i = i.wrapping_add(1);

            let cage = map.get_cage(black_box(id));
            black_box(cage);
        });
    });
}

// Measures concurrent read performance when multiple threads repeatedly
// access existing cage entries.
fn bench_parallel_get<M: CageMap>(c: &mut Criterion, name: &str) {
    let map = Arc::new(setup_map::<M>());
    let thread_count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    c.bench_function(name, |b| {
        b.iter(|| {
            let mut handles = Vec::new();

            for tid in 0..thread_count {
                let map = Arc::clone(&map);

                handles.push(thread::spawn(move || {
                    let mut sum = 0u64;

                    for i in 0..100_000 {
                        let id = (i + tid) % LIVE_CAGES;
                        if let Some(cage) = map.get_cage(id) {
                            sum = sum.wrapping_add(cage.payload);
                        }
                    }

                    black_box(sum);
                }));
            }

            for h in handles {
                h.join().unwrap();
            }
        });
    });
}

// Measures a read-mostly workload with one writer thread occasionally adding
// and removing cage entries. This is meant to approximate a cage table with
// frequent reads and relatively infrequent lifecycle updates.
fn bench_read_mostly_with_mutation<M: CageMap>(c: &mut Criterion, name: &str) {
    let map = Arc::new(setup_map::<M>());
    let thread_count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    c.bench_function(name, |b| {
        b.iter(|| {
            let mut handles = Vec::new();

            // Reader threads
            for tid in 0..thread_count {
                let map = Arc::clone(&map);

                handles.push(thread::spawn(move || {
                    let mut sum = 0u64;

                    for i in 0..100_000 {
                        let id = (i + tid) % LIVE_CAGES;
                        if let Some(cage) = map.get_cage(id) {
                            sum = sum.wrapping_add(cage.payload);
                        }
                    }

                    black_box(sum);
                }));
            }

            // One writer thread: occasional remove/add on high ids
            {
                let map = Arc::clone(&map);

                handles.push(thread::spawn(move || {
                    for i in 0..10_000 {
                        let id = LIVE_CAGES + (i % 128);

                        map.add_cage(
                            id,
                            MockCage {
                                cageid: id as u64,
                                payload: i as u64,
                            },
                        );

                        black_box(map.get_cage(id));

                        map.remove_cage(id);
                    }
                }));
            }

            for h in handles {
                h.join().unwrap();
            }
        });
    });
}

// Measures contention when all reader threads repeatedly access the same slot.
// This highlights the behavior of each implementation under worst-case
// hot-entry access.
fn bench_same_slot_contention<M: CageMap>(c: &mut Criterion, name: &str) {
    let map = Arc::new(M::new(TABLE_SIZE));
    map.add_cage(
        0,
        MockCage {
            cageid: 0,
            payload: 42,
        },
    );

    let thread_count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    c.bench_function(name, |b| {
        b.iter(|| {
            let mut handles = Vec::new();

            for _ in 0..thread_count {
                let map = Arc::clone(&map);

                handles.push(thread::spawn(move || {
                    let mut sum = 0u64;

                    for _ in 0..100_000 {
                        if let Some(cage) = map.get_cage(0) {
                            sum = sum.wrapping_add(cage.payload);
                        }
                    }

                    black_box(sum);
                }));
            }

            for h in handles {
                h.join().unwrap();
            }
        });
    });
}

// Register all cage table implementation benchmarks.
fn all_benches(c: &mut Criterion) {
    bench_single_thread_get::<GlobalRwLockMap>(c, "single_get/global_rwlock");
    bench_single_thread_get::<SlotRwLockMap>(c, "single_get/slot_rwlock");
    bench_single_thread_get::<DashMapCageMap>(c, "single_get/dashmap");
    bench_single_thread_get::<ArcSwapCageMap>(c, "single_get/arcswap");

    bench_parallel_get::<GlobalRwLockMap>(c, "parallel_get/global_rwlock");
    bench_parallel_get::<SlotRwLockMap>(c, "parallel_get/slot_rwlock");
    bench_parallel_get::<DashMapCageMap>(c, "parallel_get/dashmap");
    bench_parallel_get::<ArcSwapCageMap>(c, "parallel_get/arcswap");

    bench_read_mostly_with_mutation::<GlobalRwLockMap>(c, "read_mostly_mutation/global_rwlock");
    bench_read_mostly_with_mutation::<SlotRwLockMap>(c, "read_mostly_mutation/slot_rwlock");
    bench_read_mostly_with_mutation::<DashMapCageMap>(c, "read_mostly_mutation/dashmap");
    bench_read_mostly_with_mutation::<ArcSwapCageMap>(c, "read_mostly_mutation/arcswap");

    bench_same_slot_contention::<GlobalRwLockMap>(c, "same_slot/global_rwlock");
    bench_same_slot_contention::<SlotRwLockMap>(c, "same_slot/slot_rwlock");
    bench_same_slot_contention::<DashMapCageMap>(c, "same_slot/dashmap");
    bench_same_slot_contention::<ArcSwapCageMap>(c, "same_slot/arcswap");
}

criterion_group!(benches, all_benches);
criterion_main!(benches);
