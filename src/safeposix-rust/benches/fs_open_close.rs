/* Benchmarks for the microvisor implementation.  In general, I'm not doing
 * results checking / assertations to avoid adding bias to the results.  */

// I hate allowing this, but this is apparently a known issue for a lot of
// code with CStrings.  https://github.com/rust-lang/rust/issues/78691
// I've tried to sanity check where this occurs, but please, please, please
// double check these parts of the code!
#![allow(temporary_cstring_as_ptr)]

use criterion::{criterion_group, criterion_main, Criterion};

use rustposix::interface;

use std::ffi::*;

use rustposix::safeposix::cage::*;

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

    // --- COMPARING open / close CALLS ACROSS Lind + Native OS kernel ---
    let mut group = c.benchmark_group("Compare fs:open+close");

    // Should be similar.  Use a linear scale...
    group.plot_config(
        criterion::PlotConfiguration::default().summary_scale(criterion::AxisScale::Linear),
    );

    // Let's see how fast various file system calls are
    group.bench_function("TF01: Lind open+close", |b| {
        b.iter(|| {
            let fd = cage.open_syscall("foo", O_CREAT | O_TRUNC | O_WRONLY, S_IRWXA);
            cage.close_syscall(fd);
        })
    });

    // For comparison let's time the native OS...
    group.bench_function("TF01: Native OS kernel open+close", |b| {
        b.iter(|| unsafe {
            let fd = libc::open(
                CString::new("/tmp/foo").unwrap().as_ptr(),
                O_CREAT | O_TRUNC | O_WRONLY,
                S_IRWXA,
            );
            libc::close(fd);
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
