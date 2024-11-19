/* Benchmarks for the microvisor implementation.  In general, I'm not doing
 * results checking / assertations to avoid adding bias to the results.  */

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

use rustposix::interface;

use std::ffi::*;

use std::time::Duration;

use rustposix::safeposix::cage::*;

use rustposix::tests;

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

    // --- COMPARING read + write + lseek CALLS ACROSS Lind + Native OS kernel ---
    let mut group = c.benchmark_group("Compare fs:write+read+lseek");

    // Should be similar.  Use a linear scale...
    group.plot_config(
        criterion::PlotConfiguration::default().summary_scale(criterion::AxisScale::Linear),
    );

    // Reduce the time to go faster! Default is 5s...
    group.measurement_time(Duration::from_secs(2));

    // Shorten the warm up time as well from 3s to this...
    group.warm_up_time(Duration::from_secs(1));

    //   --- Lind  ---

    // Iterate for different buffer sizes...
    for buflen in [1, 64, 1024, 65536].iter() {
        let fd = cage.open_syscall("foo", O_CREAT | O_TRUNC | O_WRONLY, S_IRWXA);

        let deststring = tests::str2cbuf(
            &String::from_utf8(vec![b'X'; *buflen]).expect("error building string"),
        );

        let read_buffer = tests::sizecbuf(*buflen).as_mut_ptr();
        // Let's see how fast various file system calls are
        group.bench_with_input(
            BenchmarkId::new("TF03:Lind write+read+lseek", buflen),
            buflen,
            |b, buflen| {
                b.iter(|| {
                    let _ = cage.write_syscall(fd, deststring, *buflen);
                    cage.lseek_syscall(fd, 0, SEEK_SET);
                    cage.read_syscall(fd, read_buffer, *buflen);
                    cage.lseek_syscall(fd, 0, SEEK_SET);
                })
            },
        );

        cage.close_syscall(fd);
        cage.unlink_syscall("foo");
    }

    //   --- Native  ---

    // Iterate for different buffer sizes...
    for buflen in [1, 64, 1024, 65536].iter() {
        let fd: c_int;

        let c_str = CString::new("/tmp/foo").unwrap();

        let path = c_str.into_raw() as *const u8;

        unsafe {
            fd = libc::open(path, O_CREAT | O_TRUNC | O_WRONLY, S_IRWXA);
        }

        let deststring = tests::str2cbuf(
            &String::from_utf8(vec![b'X'; *buflen]).expect("error building string"),
        );

        let read_buffer = tests::sizecbuf(*buflen).as_mut_ptr();

        // For comparison let's time the native OS...
        group.bench_with_input(
            BenchmarkId::new("TF03:Native write+read+lseek", buflen),
            buflen,
            |b, buflen| {
                b.iter(|| unsafe {
                    let _ = libc::write(fd, deststring as *const c_void, *buflen);
                    libc::lseek(fd, 0, SEEK_SET);
                    libc::read(fd, read_buffer as *mut c_void, *buflen);
                    libc::lseek(fd, 0, SEEK_SET);
                })
            },
        );
        unsafe {
            libc::close(fd);
            libc::unlink(path);
        }
    }

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
