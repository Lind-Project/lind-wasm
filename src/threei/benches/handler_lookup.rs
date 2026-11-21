use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::hint::black_box;
use std::thread;
use std::time::{Duration, Instant};
use threei::handler_table::{
    _check_cage_handler_exists, _get_handler, _rm_cage_from_handler,
    copy_handler_table_to_cage_impl, register_handler_impl, HANDLERTABLE,
};
use threei::threei_const::THREEI_DEREGISTER;
use threei::{
    get_runtime_cleanup_funcptr, get_runtime_trampoline, register_trampoline, EXITING_TABLE,
    TRAMPOLINE_TABLE,
};

const CAGE_COUNT: u64 = 1_024;
const SYSCALL_BASE: u64 = 10_000;
const HANDLER_CAGE_BASE: u64 = 50_000;
const HANDLER_ADDR_BASE: u64 = 0x1000_0000;
const COPY_SOURCE_CAGE: u64 = 70_000;
const COPY_TARGET_CAGE_BASE: u64 = 80_000;
const COPY_CALL_COUNT: u64 = 256;
const RUNTIME_COUNT: u64 = 1_024;
const RUNTIME_BASE: u64 = 90_000;

extern "C" fn noop_trampoline(
    _in_grate_fn_ptr_u64: u64,
    _grateid: u64,
    _arg1: u64,
    _arg1cageid: u64,
    _arg2: u64,
    _arg2cageid: u64,
    _arg3: u64,
    _arg3cageid: u64,
    _arg4: u64,
    _arg4cageid: u64,
    _arg5: u64,
    _arg5cageid: u64,
    _arg6: u64,
    _arg6cageid: u64,
) -> i32 {
    0
}

fn clear_globals() {
    #[cfg(feature = "hashmap")]
    {
        HANDLERTABLE.lock().unwrap().clear();
    }

    #[cfg(feature = "dashmap")]
    {
        HANDLERTABLE.clear();
    }

    EXITING_TABLE.clear();
    TRAMPOLINE_TABLE.clear();
}

fn populate_handler_table(cage_count: u64) {
    clear_globals();

    for cageid in 1..=cage_count {
        let syscall_num = SYSCALL_BASE + cageid;
        let handler_cageid = HANDLER_CAGE_BASE + cageid;
        let handler_addr = HANDLER_ADDR_BASE + cageid;

        assert_eq!(
            register_handler_impl(cageid, syscall_num, handler_cageid, handler_addr),
            0
        );
    }
}

fn populate_copy_source(call_count: u64) {
    clear_globals();

    for call_index in 0..call_count {
        assert_eq!(
            register_handler_impl(
                COPY_SOURCE_CAGE,
                SYSCALL_BASE + call_index,
                HANDLER_CAGE_BASE + call_index,
                HANDLER_ADDR_BASE + call_index,
            ),
            0
        );
    }
}

fn populate_trampoline_table(runtime_count: u64) {
    clear_globals();

    for runtime in RUNTIME_BASE..RUNTIME_BASE + runtime_count {
        register_trampoline(runtime, noop_trampoline, HANDLER_ADDR_BASE + runtime);
    }
}

fn bench_single_hot_lookup(c: &mut Criterion) {
    populate_handler_table(CAGE_COUNT);

    let cageid = CAGE_COUNT / 2;
    let syscall_num = SYSCALL_BASE + cageid;
    let expected = (HANDLER_CAGE_BASE + cageid, HANDLER_ADDR_BASE + cageid);

    c.bench_function("handler_lookup/single_hot_hit", |b| {
        b.iter(|| {
            let handler =
                _get_handler(black_box(cageid), black_box(syscall_num), black_box(cageid)).unwrap();
            assert_eq!(handler, expected);
            black_box(handler);
        });
    });
}

fn bench_many_cage_lookup(c: &mut Criterion) {
    populate_handler_table(CAGE_COUNT);

    let mut next = 1_u64;

    c.bench_function("handler_lookup/many_cage_hits", |b| {
        b.iter(|| {
            let cageid = next;
            next += 1;
            if next > CAGE_COUNT {
                next = 1;
            }

            let syscall_num = SYSCALL_BASE + cageid;
            let handler =
                _get_handler(black_box(cageid), black_box(syscall_num), black_box(cageid)).unwrap();

            assert_eq!(handler.0, HANDLER_CAGE_BASE + cageid);
            assert_eq!(handler.1, HANDLER_ADDR_BASE + cageid);
            black_box(handler);
        });
    });
}

fn bench_cage_exists_lookup(c: &mut Criterion) {
    populate_handler_table(CAGE_COUNT);

    c.bench_function("handler_lookup/cage_exists_hit", |b| {
        b.iter(|| {
            let exists = _check_cage_handler_exists(black_box(CAGE_COUNT / 2));
            assert!(exists);
            black_box(exists);
        });
    });

    c.bench_function("handler_lookup/cage_exists_miss", |b| {
        b.iter(|| {
            let exists = _check_cage_handler_exists(black_box(CAGE_COUNT + 1));
            assert!(!exists);
            black_box(exists);
        });
    });
}

fn bench_parallel_lookup(c: &mut Criterion) {
    populate_handler_table(CAGE_COUNT);

    let threads = thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(4)
        .min(8);

    c.bench_function("handler_lookup/parallel_hits", |b| {
        b.iter_custom(|iters| {
            if iters == 0 {
                return Duration::ZERO;
            }

            let base_iters = iters / threads as u64;
            let extra_iters = iters % threads as u64;

            let start = Instant::now();
            thread::scope(|scope| {
                for thread_id in 0..threads {
                    let count = base_iters + u64::from((thread_id as u64) < extra_iters);

                    scope.spawn(move || {
                        for i in 0..count {
                            let cageid = ((i + thread_id as u64) % CAGE_COUNT) + 1;
                            let syscall_num = SYSCALL_BASE + cageid;
                            let handler = _get_handler(
                                black_box(cageid),
                                black_box(syscall_num),
                                black_box(cageid),
                            )
                            .unwrap();

                            assert_eq!(handler.0, HANDLER_CAGE_BASE + cageid);
                            assert_eq!(handler.1, HANDLER_ADDR_BASE + cageid);
                            black_box(handler);
                        }
                    });
                }
            });
            start.elapsed()
        });
    });
}

fn bench_register_handler(c: &mut Criterion) {
    clear_globals();

    let mut next = 1_u64;

    c.bench_function("handler_table/register_new_handler", |b| {
        b.iter_batched(
            || {
                let cageid = next;
                next += 1;
                if next > CAGE_COUNT {
                    next = 1;
                }

                _rm_cage_from_handler(cageid);
                cageid
            },
            |cageid| {
                let ret = register_handler_impl(
                    black_box(cageid),
                    black_box(SYSCALL_BASE + cageid),
                    black_box(HANDLER_CAGE_BASE + cageid),
                    black_box(HANDLER_ADDR_BASE + cageid),
                );
                assert_eq!(ret, 0);
                black_box(ret);
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_overwrite_handler(c: &mut Criterion) {
    clear_globals();

    let cageid = 1;
    let syscall_num = SYSCALL_BASE + cageid;
    assert_eq!(
        register_handler_impl(cageid, syscall_num, HANDLER_CAGE_BASE, HANDLER_ADDR_BASE),
        0
    );

    let mut next_addr = HANDLER_ADDR_BASE;

    c.bench_function("handler_table/overwrite_existing_handler", |b| {
        b.iter(|| {
            next_addr += 1;
            let ret = register_handler_impl(
                black_box(cageid),
                black_box(syscall_num),
                black_box(HANDLER_CAGE_BASE),
                black_box(next_addr),
            );
            assert_eq!(ret, 0);
            black_box(ret);
        });
    });
}

fn bench_deregister_handler(c: &mut Criterion) {
    clear_globals();

    let mut next = 1_u64;

    c.bench_function("handler_table/deregister_callnum", |b| {
        b.iter_batched(
            || {
                let cageid = next;
                next += 1;
                if next > CAGE_COUNT {
                    next = 1;
                }

                let syscall_num = SYSCALL_BASE + cageid;
                assert_eq!(
                    register_handler_impl(
                        cageid,
                        syscall_num,
                        HANDLER_CAGE_BASE,
                        HANDLER_ADDR_BASE
                    ),
                    0
                );
                (cageid, syscall_num)
            },
            |(cageid, syscall_num)| {
                let ret = register_handler_impl(
                    black_box(cageid),
                    black_box(syscall_num),
                    black_box(THREEI_DEREGISTER),
                    black_box(0),
                );
                assert_eq!(ret, 0);
                black_box(ret);
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_copy_handler_table(c: &mut Criterion) {
    populate_copy_source(COPY_CALL_COUNT);

    let mut next_target = COPY_TARGET_CAGE_BASE;

    c.bench_function("handler_table/copy_handler_table_256_callnums", |b| {
        b.iter_batched(
            || {
                let target = next_target;
                next_target += 1;
                if next_target >= COPY_TARGET_CAGE_BASE + CAGE_COUNT {
                    next_target = COPY_TARGET_CAGE_BASE;
                }

                _rm_cage_from_handler(target);
                target
            },
            |target| {
                let ret =
                    copy_handler_table_to_cage_impl(black_box(COPY_SOURCE_CAGE), black_box(target));
                assert_eq!(ret, 0);
                black_box(ret);
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_runtime_trampoline_lookup(c: &mut Criterion) {
    populate_trampoline_table(RUNTIME_COUNT);

    let mut next = 0_u64;

    c.bench_function("runtime_trampoline/lookup_trampoline", |b| {
        b.iter(|| {
            let runtime = RUNTIME_BASE + (next % RUNTIME_COUNT);
            next += 1;

            let trampoline = get_runtime_trampoline(black_box(runtime)).unwrap();
            assert_eq!(
                trampoline(0, runtime, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0),
                0
            );
            black_box(trampoline);
        });
    });

    let mut next_cleanup = 0_u64;

    c.bench_function("runtime_trampoline/lookup_cleanup_funcptr", |b| {
        b.iter(|| {
            let runtime = RUNTIME_BASE + (next_cleanup % RUNTIME_COUNT);
            next_cleanup += 1;

            let cleanup = get_runtime_cleanup_funcptr(black_box(runtime)).unwrap();
            assert_eq!(cleanup, HANDLER_ADDR_BASE + runtime);
            black_box(cleanup);
        });
    });
}

fn bench_register_trampoline(c: &mut Criterion) {
    clear_globals();

    let mut next_runtime = RUNTIME_BASE;

    c.bench_function("runtime_trampoline/register_trampoline", |b| {
        b.iter_batched(
            || {
                let runtime = next_runtime;
                next_runtime += 1;
                if next_runtime >= RUNTIME_BASE + RUNTIME_COUNT {
                    next_runtime = RUNTIME_BASE;
                }

                TRAMPOLINE_TABLE.remove(&runtime);
                runtime
            },
            |runtime| {
                register_trampoline(
                    black_box(runtime),
                    black_box(noop_trampoline),
                    black_box(HANDLER_ADDR_BASE + runtime),
                );
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(
    benches,
    bench_single_hot_lookup,
    bench_many_cage_lookup,
    bench_cage_exists_lookup,
    bench_parallel_lookup,
    bench_register_handler,
    bench_overwrite_handler,
    bench_deregister_handler,
    bench_copy_handler_table,
    bench_runtime_trampoline_lookup,
    bench_register_trampoline
);
criterion_main!(benches);
