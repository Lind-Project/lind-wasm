use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};

use threei::threei::threei::*;
use threei::cage::*;
use threei::rawposix::vmmap::*;
use threei::fdtables;

const FDKIND_KERNEL: u32 = 0;

/// Helper function to initialize a cage
fn simple_init_cage(cageid: u64) {
    let cage = Cage {
        cageid: cageid,
        cwd: RwLock::new(Arc::new(PathBuf::from("/"))),
        parent: 1,
        gid: AtomicI32::new(-1),
        uid: AtomicI32::new(-1),
        egid: AtomicI32::new(-1),
        euid: AtomicI32::new(-1),
        main_threadid: AtomicU64::new(0),
        zombies: RwLock::new(vec![]),
        child_num: AtomicU64::new(0),
        vmmap: RwLock::new(Vmmap::new())
    };
    add_cage(cage);
    fdtables::init_empty_cage(cageid);
    fdtables::get_specific_virtual_fd(cageid, 0, FDKIND_KERNEL, 0, false, 0).unwrap();
    fdtables::get_specific_virtual_fd(cageid, 1, FDKIND_KERNEL, 1, false, 0).unwrap();
    fdtables::get_specific_virtual_fd(cageid, 2, FDKIND_KERNEL, 2, false, 0).unwrap();
}

/// make benchmark -- caller and callee are same 
/// Step the workload up from 1 to 100 times and measure the runtime for each workload
fn benchmark_make_same_syscall(c: &mut Criterion) {
    let mut group = c.benchmark_group("make_syscall");

    // Initialize cages
    for cage_id in 1..=100 {
        simple_init_cage(cage_id);
    }

    for num_cages in 1..=100 as u64 {
        group.bench_with_input(BenchmarkId::from_parameter(num_cages), &num_cages, |b, &num_cages| {
            let cage_ids: Vec<u64> = (1..=num_cages as u64).collect();
            
            // b.iter() will choose the loop times according to the test behaviors.. not sure if we need to handle manually
            b.iter(|| {
                for &cage_id in &cage_ids {
                    let _ = make_syscall(cage_id, 0, 1, cage_id, 0, 0, 0, 0, 0, 0);
                }
            });

        });
    }

    group.finish();
}

/// Registers for every: cage_id/write --> cage_id + 1/hello
fn benchmark_register_syscall(c: &mut Criterion) {
    let mut group = c.benchmark_group("register_syscall");

    // We use cages initialized in the first benchmark, so no need to re-initialization

    for num_cages in 1..=99 as u64 {
        group.bench_with_input(BenchmarkId::from_parameter(num_cages), &num_cages, |b, &num_cages| {
            let cage_ids: Vec<u64> = (1..=num_cages as u64).collect();

            // register handler for different cages
            
            b.iter(|| {
                for &cage_id in &cage_ids {
                    let _ = register_handler(
                        0,                  // Unused, kept for syscall convention
                        cage_id+1,                // target cageid: next one
                        1,             // target syscall: hello 
                        0,                 // Unused 
                        2,                // self syscall: write
                        cage_id,            // self cageid this one
                        0, 0, 0, 0, 0, 0, 0, 0,             // Unused 
                    );
                }
            });

        });
    }

    group.finish();
}

/// Different caller and callee
fn benchmark_make_different_syscall(c: &mut Criterion) {
    let mut group = c.benchmark_group("make_syscall_different");

    // We use cages initialized in first benchmark, so no need to re-initialization

    // Then handler has been registered

    for num_cages in 1..=100 as u64 {
        group.bench_with_input(BenchmarkId::from_parameter(num_cages), &num_cages, |b, &num_cages| {
            let cage_ids: Vec<u64> = (1..=num_cages as u64).collect();
            // Call from cage_id to cage_id+1(next cage)
            b.iter(|| {
                for &cage_id in &cage_ids {
                    let _ = make_syscall(
                        cage_id, 
                        2, 
                        1,
                        cage_id+1, 
                        0, 0, 0, 0, 0, 0,
                    );
                }
            });

        });
    }

    group.finish();
}

/// Call syscall at the end of call stack 
// fn benchmark_only_make_last(c: &mut Criterion) {

// }


criterion_group!(
    benches_exit, 
    benchmark_make_same_syscall, 
    benchmark_register_syscall, 
    benchmark_make_different_syscall,
);
criterion_main!(benches_exit);
