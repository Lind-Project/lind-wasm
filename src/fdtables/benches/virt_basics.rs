/* Benchmarks for fdtables.  This does a few basic operations related to
 * virtual fd -> real fd translation */

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

use fdtables::*;

use std::thread;

use std::time::Duration;

pub fn run_benchmark(c: &mut Criterion) {
    // I'm going to do some simple calls using fdtables in this file
    let mut group = c.benchmark_group("fdtables basics");

    // Reduce the time to reduce disk space needed and go faster.
    // Default is 5s...
    group.measurement_time(Duration::from_secs(3));

    // Shorten the warm up time as well from 3s to this...
    group.warm_up_time(Duration::from_secs(1));

    let fd1 = get_unused_virtual_fd(threei::TESTING_CAGEID, 10, true, 100).unwrap();
    let fd2 = get_unused_virtual_fd(threei::TESTING_CAGEID, 20, true, 1).unwrap();
    let fd3 = get_unused_virtual_fd(threei::TESTING_CAGEID, 30, true, 10).unwrap();

    // I'm going to insert three items, then do 10000 queries, then clean up...
    group.bench_function(format!("{}/st: trans (10K)", ALGONAME), |b| {
        b.iter(|| {
            for _ in 0..1000 {
                translate_virtual_fd(threei::TESTING_CAGEID, fd1).unwrap();
                translate_virtual_fd(threei::TESTING_CAGEID, fd2).unwrap();
                translate_virtual_fd(threei::TESTING_CAGEID, fd3).unwrap();
                translate_virtual_fd(threei::TESTING_CAGEID, fd1).unwrap();
                translate_virtual_fd(threei::TESTING_CAGEID, fd2).unwrap();
                translate_virtual_fd(threei::TESTING_CAGEID, fd3).unwrap();
                translate_virtual_fd(threei::TESTING_CAGEID, fd1).unwrap();
                translate_virtual_fd(threei::TESTING_CAGEID, fd2).unwrap();
                translate_virtual_fd(threei::TESTING_CAGEID, fd3).unwrap();
                translate_virtual_fd(threei::TESTING_CAGEID, fd1).unwrap();
            }
        })
    });

    refresh();

    // only do 1000 because 1024 is a common lower bound
    group.bench_function(format!("{}/st: getvirt (1K)", ALGONAME), |b| {
        b.iter(|| {
            for _ in 0..1000 {
                _ = get_unused_virtual_fd(threei::TESTING_CAGEID, 30, true, 10).unwrap();
            }
            // unfortunately, we need to clean up, or else we will
            // get an exception due to the table being full...
            refresh();
        })
    });

    // Check get_optionalinfo...
    let fd = get_unused_virtual_fd(threei::TESTING_CAGEID, 30, true, 10).unwrap();
    group.bench_function(format!("{}/st: get_opt (10K)", ALGONAME), |b| {
        b.iter(|| {
            for _ in 0..10000 {
                _ = get_optionalinfo(threei::TESTING_CAGEID, fd).unwrap();
            }
        })
    });

    refresh();

    // flip the set_optionalinfo data...
    let fd = get_unused_virtual_fd(threei::TESTING_CAGEID, 30, true, 10).unwrap();
    group.bench_function(format!("{}/st: set_opt (10K)", ALGONAME), |b| {
        b.iter(|| {
            for _ in 0..5000 {
                _ = set_optionalinfo(threei::TESTING_CAGEID, fd, 100).unwrap();
                _ = set_optionalinfo(threei::TESTING_CAGEID, fd, 200).unwrap();
            }
        })
    });

    refresh();

    // TODO: I'd love to count memory use in these tests too.  It really
    // varies widely...

    // check copy_fdtable_for_cage (fork) time...
    for fdcount in [1, 4, 16, 64, 256, 1024].iter() {
        // Setup the fds up front, outside of the benchmark...
        for _ in 0..*fdcount {
            let _fd = get_unused_virtual_fd(threei::TESTING_CAGEID, 30, false, 10).unwrap();
        }
        let mut cagenumtouse = 1;
        group.bench_with_input(
            BenchmarkId::new(format!("{}/st: fork (fds:{})", ALGONAME, fdcount), fdcount),
            fdcount,
            |b, _fdcount| {
                b.iter({
                    || {
                        copy_fdtable_for_cage(threei::TESTING_CAGEID, cagenumtouse).unwrap();
                        // Get a new cage each time...
                        cagenumtouse += 1;
                        // The number of cages may grow large and this could
                        // also skew the results...  Reset after 100...
                        //
                        // Also, if I ever get around to limiting the global
                        // fds, this will panic...
                        if cagenumtouse % 100 == 0 {
                            refresh();
                        }
                    }
                })
            },
        );
        refresh();
    }
    refresh();

    // check remove_cage_from_fdtable (exit) time...
    for fdcount in [1, 4, 16, 64, 256, 1024].iter() {
        group.bench_with_input(
            BenchmarkId::new(format!("{}/st: exit (fds:{})", ALGONAME, fdcount), fdcount),
            fdcount,
            |b, _fdcount| {
                b.iter({
                    || {
                        // BUG: Is there a better way to do this?  I really
                        // only want to check the empty_fds_for_exec() call
                        // time...
                        for _ in 0..*fdcount {
                            let _fd = get_unused_virtual_fd(threei::TESTING_CAGEID, 30, false, 10)
                                .unwrap();
                        }
                        remove_cage_from_fdtable(threei::TESTING_CAGEID);
                        // need to re-add the cage...
                        refresh();
                    }
                })
            },
        );
    }
    refresh();

    // check on empty_fds_for_exec with the flag set to false...
    for fdcount in [1, 4, 16, 64, 256, 1024].iter() {
        group.bench_with_input(
            BenchmarkId::new(
                format!("{}/st: exec (false) (fds:{})", ALGONAME, fdcount),
                fdcount,
            ),
            fdcount,
            |b, fdcount| {
                b.iter({
                    || {
                        // BUG: Is there a better way to do this?  I really
                        // only want to check the empty_fds_for_exec() call
                        // time...
                        for _ in 0..*fdcount {
                            let _fd = get_unused_virtual_fd(threei::TESTING_CAGEID, 30, false, 10)
                                .unwrap(); // Notice the false here!
                        }
                        empty_fds_for_exec(threei::TESTING_CAGEID);
                        refresh();
                    }
                })
            },
        );
    }
    refresh();

    // Now, check on empty_fds_for_exec with the flag set to true...
    for fdcount in [1, 4, 16, 64, 256, 1024].iter() {
        group.bench_with_input(
            BenchmarkId::new(
                format!("{}/st: exec (true) (fds:{})", ALGONAME, fdcount),
                fdcount,
            ),
            fdcount,
            |b, fdcount| {
                b.iter({
                    || {
                        // BUG: Is there a better way to do this?  I really
                        // only want to check the empty_fds_for_exec() call
                        // time...
                        for _ in 0..*fdcount {
                            let _fd = get_unused_virtual_fd(threei::TESTING_CAGEID, 30, true, 10)
                                .unwrap(); // Notice the true here!
                        }
                        empty_fds_for_exec(threei::TESTING_CAGEID);
                        //refresh(); <- Don't need this because the prior
                        // line cleans up for me!
                    }
                })
            },
        );
    }
    refresh();

    // ---------------- MULTI-THREADED / 1 cage TESTS ------------------  //

    // -- Multithreaded benchmark 1: 100K translate calls --

    let fd = get_unused_virtual_fd(threei::TESTING_CAGEID, 10, true, 100).unwrap();
    let fd2 = get_unused_virtual_fd(threei::TESTING_CAGEID, 20, true, 200).unwrap();
    let fd3 = get_unused_virtual_fd(threei::TESTING_CAGEID, 30, true, 300).unwrap();

    for threadcount in [1, 2, 4, 8, 16].iter() {
        group.bench_with_input(
            BenchmarkId::new(
                format!("{}/[mt1c:{}] trans_virtfd (100K)", ALGONAME, threadcount),
                threadcount,
            ),
            threadcount,
            |b, threadcount| {
                b.iter({
                    || {
                        let mut thread_handle_vec: Vec<thread::JoinHandle<()>> = Vec::new();
                        for _numthreads in 0..*threadcount {
                            // Need to borrow so the lifetime can live outside
                            // the thread's closure
                            let thisthreadcount = *threadcount;

                            thread_handle_vec.push(thread::spawn(move || {
                                // Do 10K / threadcount of 10 requests each.  100K total
                                for _ in 0..10000 / thisthreadcount {
                                    translate_virtual_fd(threei::TESTING_CAGEID, fd).unwrap();
                                    translate_virtual_fd(threei::TESTING_CAGEID, fd).unwrap();
                                    translate_virtual_fd(threei::TESTING_CAGEID, fd).unwrap();
                                    translate_virtual_fd(threei::TESTING_CAGEID, fd).unwrap();
                                    translate_virtual_fd(threei::TESTING_CAGEID, fd2).unwrap();
                                    translate_virtual_fd(threei::TESTING_CAGEID, fd2).unwrap();
                                    translate_virtual_fd(threei::TESTING_CAGEID, fd2).unwrap();
                                    translate_virtual_fd(threei::TESTING_CAGEID, fd3).unwrap();
                                    translate_virtual_fd(threei::TESTING_CAGEID, fd3).unwrap();
                                    translate_virtual_fd(threei::TESTING_CAGEID, fd3).unwrap();
                                }
                            }));
                        }
                        for handle in thread_handle_vec {
                            handle.join().unwrap();
                        }
                    }
                })
            },
        );
    }
    refresh();

    // -- Multithreaded benchmark 2: get / translate interleaved --

    // I will always do 100K requests (split amongst some number of threads)

    for threadcount in [1, 2, 4, 8, 16].iter() {
        group.bench_with_input(
            BenchmarkId::new(
                format!("{}/[mt1c:{}] get_trans (1K per)", ALGONAME, threadcount),
                threadcount,
            ),
            threadcount,
            |b, threadcount| {
                b.iter({
                    || {
                        let mut thread_handle_vec: Vec<thread::JoinHandle<()>> = Vec::new();
                        for _numthreads in 0..*threadcount {
                            // Need to borrow so the lifetime can live outside
                            // the thread's closure
                            let thisthreadcount = *threadcount;

                            thread_handle_vec.push(thread::spawn(move || {
                                // Do 1K / threadcount
                                for _ in 0..1000 / thisthreadcount {
                                    let fd = get_unused_virtual_fd(
                                        threei::TESTING_CAGEID,
                                        10,
                                        true,
                                        100,
                                    )
                                    .unwrap();
                                    translate_virtual_fd(threei::TESTING_CAGEID, fd).unwrap();
                                }
                            }));
                        }
                        for handle in thread_handle_vec {
                            handle.join().unwrap();
                        }
                        refresh();
                    }
                })
            },
        );
    }

    // -- Multithreaded benchmark 3: get / close interleaved --

    // I will always do 100K requests (split amongst some number of threads)

    for threadcount in [1, 2, 4, 8, 16].iter() {
        group.bench_with_input(
            BenchmarkId::new(
                format!("{}/[mt1c:{}] get_close (10K)", ALGONAME, threadcount),
                threadcount,
            ),
            threadcount,
            |b, threadcount| {
                b.iter({
                    || {
                        let mut thread_handle_vec: Vec<thread::JoinHandle<()>> = Vec::new();
                        for _numthreads in 0..*threadcount {
                            // Need to borrow so the lifetime can live outside
                            // the thread's closure
                            let thisthreadcount = *threadcount;

                            thread_handle_vec.push(thread::spawn(move || {
                                // Do 100K / threadcount each
                                for _ in 0..10000 / thisthreadcount {
                                    let fd = get_unused_virtual_fd(
                                        threei::TESTING_CAGEID,
                                        10,
                                        true,
                                        100,
                                    )
                                    .unwrap();
                                    close_virtualfd(threei::TESTING_CAGEID, fd).unwrap();
                                }
                            }));
                        }
                        for handle in thread_handle_vec {
                            handle.join().unwrap();
                        }
                    }
                })
            },
        );
    }

    // -------------- MULTI-THREADED / MULTI-CAGE TESTS ----------------  //

    // -- Multithreaded benchmark 1: 100K translate calls --

    let fd = get_unused_virtual_fd(threei::TESTING_CAGEID, 10, true, 100).unwrap();
    let fd2 = get_unused_virtual_fd(threei::TESTING_CAGEID, 20, true, 200).unwrap();
    let fd3 = get_unused_virtual_fd(threei::TESTING_CAGEID, 30, true, 300).unwrap();
    for val in 1..16 {
        // I'm just going to assume I can increment these...
        copy_fdtable_for_cage(threei::TESTING_CAGEID, threei::TESTING_CAGEID + val).unwrap();
    }

    for threadcount in [1, 2, 4, 8, 16].iter() {
        group.bench_with_input(
            BenchmarkId::new(
                format!("{}/[mtmc:{}] trans_virtfd (100K)", ALGONAME, threadcount),
                threadcount,
            ),
            threadcount,
            |b, threadcount| {
                b.iter({
                    || {
                        let mut thread_handle_vec: Vec<thread::JoinHandle<()>> = Vec::new();
                        for numthreads in 0..*threadcount {
                            // Need to borrow so the lifetime can live outside
                            // the thread's closure
                            let thisthreadcount = *threadcount;

                            thread_handle_vec.push(thread::spawn(move || {
                                // Do 10K / threadcount of 10 requests each.  100K total
                                for _ in 0..10000 / thisthreadcount {
                                    translate_virtual_fd(threei::TESTING_CAGEID + numthreads, fd)
                                        .unwrap();
                                    translate_virtual_fd(threei::TESTING_CAGEID + numthreads, fd)
                                        .unwrap();
                                    translate_virtual_fd(threei::TESTING_CAGEID + numthreads, fd)
                                        .unwrap();
                                    translate_virtual_fd(threei::TESTING_CAGEID + numthreads, fd)
                                        .unwrap();
                                    translate_virtual_fd(threei::TESTING_CAGEID + numthreads, fd2)
                                        .unwrap();
                                    translate_virtual_fd(threei::TESTING_CAGEID + numthreads, fd2)
                                        .unwrap();
                                    translate_virtual_fd(threei::TESTING_CAGEID + numthreads, fd2)
                                        .unwrap();
                                    translate_virtual_fd(threei::TESTING_CAGEID + numthreads, fd3)
                                        .unwrap();
                                    translate_virtual_fd(threei::TESTING_CAGEID + numthreads, fd3)
                                        .unwrap();
                                    translate_virtual_fd(threei::TESTING_CAGEID + numthreads, fd3)
                                        .unwrap();
                                }
                            }));
                        }
                        for handle in thread_handle_vec {
                            handle.join().unwrap();
                        }
                    }
                })
            },
        );
    }
    refresh();

    // -- Multithreaded benchmark 2: get / translate interleaved --

    // I will always do 100K requests (split amongst some number of threads)

    for threadcount in [1, 2, 4, 8, 16].iter() {
        group.bench_with_input(
            BenchmarkId::new(
                format!("{}/[mtmc:{}] get_trans (1K per)", ALGONAME, threadcount),
                threadcount,
            ),
            threadcount,
            |b, threadcount| {
                b.iter({
                    || {
                        // setup the empty cages
                        for numthreads in 1..*threadcount {
                            copy_fdtable_for_cage(
                                threei::TESTING_CAGEID,
                                threei::TESTING_CAGEID + numthreads,
                            )
                            .unwrap();
                        }

                        let mut thread_handle_vec: Vec<thread::JoinHandle<()>> = Vec::new();
                        for numthreads in 0..*threadcount {
                            // Need to borrow so the lifetime can live outside
                            // the thread's closure
                            let thisthreadcount = *threadcount;
                            // make a copy for this cage...

                            thread_handle_vec.push(thread::spawn(move || {
                                // Do 1K / threadcount
                                for _ in 0..1000 / thisthreadcount {
                                    let fd = get_unused_virtual_fd(
                                        threei::TESTING_CAGEID + numthreads,
                                        10,
                                        true,
                                        100,
                                    )
                                    .unwrap();
                                    translate_virtual_fd(threei::TESTING_CAGEID + numthreads, fd)
                                        .unwrap();
                                }
                            }));
                        }
                        for handle in thread_handle_vec {
                            handle.join().unwrap();
                        }
                        refresh();
                    }
                })
            },
        );
    }

    refresh();

    // -- Multithreaded benchmark 3: get / close interleaved --

    // dup the cage tables as this is different cages for each...
    for val in 1..16 {
        // I'm just going to assume I can increment these...
        copy_fdtable_for_cage(threei::TESTING_CAGEID, threei::TESTING_CAGEID + val).unwrap();
    }

    // I will always do 100K requests (split amongst some number of threads)

    for threadcount in [1, 2, 4, 8, 16].iter() {
        group.bench_with_input(
            BenchmarkId::new(
                format!("{}/[mtmc:{}] get_close (10K)", ALGONAME, threadcount),
                threadcount,
            ),
            threadcount,
            |b, threadcount| {
                b.iter({
                    || {
                        let mut thread_handle_vec: Vec<thread::JoinHandle<()>> = Vec::new();
                        for _numthreads in 0..*threadcount {
                            // Need to borrow so the lifetime can live outside
                            // the thread's closure
                            let thisthreadcount = *threadcount;

                            thread_handle_vec.push(thread::spawn(move || {
                                // Do 100K / threadcount each
                                for _ in 0..10000 / thisthreadcount {
                                    let fd = get_unused_virtual_fd(
                                        threei::TESTING_CAGEID,
                                        10,
                                        true,
                                        100,
                                    )
                                    .unwrap();
                                    close_virtualfd(threei::TESTING_CAGEID, fd).unwrap();
                                }
                            }));
                        }
                        for handle in thread_handle_vec {
                            handle.join().unwrap();
                        }
                    }
                })
            },
        );
    }

    group.finish();
}

criterion_group!(benches, run_benchmark);
criterion_main!(benches);
