/* Benchmarks for the microvisor implementation.  In general, I'm not doing
 * results checking / assertations to avoid adding bias to the results.  */

use criterion::{criterion_group, criterion_main, Criterion};

use rustposix::interface;

// Using this to include my criterion settings from a single shared file.
// I did not use "use" or "mod" because benches/ isn't in the crate's usual
// namespace and I didn't want to either make a separate crate with a single,
// tiny file or add this file to the rustposix crate.
mod global_criterion_settings;

pub fn run_benchmark(c: &mut Criterion) {
    // I'm following the initialization workflow from the unit tests here.
    //
    // I'm using the lindrustinit to set up cages and the file system.
    rustposix::safeposix::dispatcher::lindrustinit(0);

    // Since all system calls are a method of a cage object, I also need this
    // reference.
    let cage = interface::cagetable_getref(1);

    // --- COMPARING get*id CALLS ACROSS Lind + Native OS kernel ---
    let mut group = c.benchmark_group("Compare get*ids");

    // These should be quite different, so use a log axis..
    group.plot_config(
        criterion::PlotConfiguration::default().summary_scale(criterion::AxisScale::Logarithmic),
    );

    // let's have a combined benchmark of all of the get*id* system calls
    // in RustPOSIX...  I'm not running these separately, because they should
    // not vary too much.
    group.bench_function("TG01: Lind get*ids", |b| {
        b.iter(|| {
            cage.getpid_syscall();
            cage.getppid_syscall();
            cage.getgid_syscall();
            cage.getegid_syscall();
            cage.getuid_syscall();
            cage.geteuid_syscall();
        })
    });
    // For comparison let's time the native OS...
    group.bench_function("TG01: Native OS kernel get*ids", |b| {
        b.iter(|| unsafe {
            libc::getpid();
            libc::getppid();
            libc::getgid();
            libc::getegid();
            libc::getuid();
            libc::geteuid();
        })
    });
    group.finish();

    // This cleans up in ways I do not fully understand.  I think it ensures
    // the file system is cleaned up
    rustposix::safeposix::dispatcher::lindrustfinalize();
}

criterion_group!(name=benches; 
                 // Add the global settings here so we don't type it everywhere
                 config=global_criterion_settings::get_criterion(); 
                 targets=run_benchmark);
criterion_main!(benches);
