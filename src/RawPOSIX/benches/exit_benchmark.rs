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

/// Registers for every: cage_id/write --> cage_id + 1/hello
/// only insert one for each
fn benchmark_register_syscall(c: &mut Criterion) {
    let mut group = c.benchmark_group("register_syscall");

    // We use cages initialized in the first benchmark, so no need to re-initialization

    for cage_id in 1..=99 as u64 {
        group.bench_with_input(BenchmarkId::from_parameter(cage_id), &cage_id, |b, &cage_id| {

            // register handler for different cages
            b.iter(|| {
                let _ = register_handler(
                    0,                  // Unused, kept for syscall convention
                    cage_id+1,                // target cageid: next one
                    1,             // target syscall: hello 
                    0,                 // Unused 
                    2,                // self syscall: write
                    cage_id,            // self cageid this one
                    0, 0, 0, 0, 0, 0, 0, 0,             // Unused 
                );
            });

        });
    }

    group.finish();
}

/// Testing exit
fn benchmark_exit(c: &mut Criterion) {
    let mut group = c.benchmark_group("exit");

    // Exit from the last one
    for cage_id in (1..=99).rev() {
        group.bench_with_input(BenchmarkId::from_parameter(cage_id), &cage_id, |b, &cage_id| {
            b.iter(|| {
                let _ = trigger_harsh_cage_exit(
                    cage_id, 
                    0,
                );
            });

        });
    }
    group.finish();
}

criterion_group!(
    benches, 
    benchmark_register_syscall,
    benchmark_exit,
);
criterion_main!(benches);