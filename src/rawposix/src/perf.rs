use lind_perf::Counter;

pub static LIBC_CALL: Counter = Counter::new("rawposix::libc_call");
pub static FDTABLES_CALL: Counter = Counter::new("rawposix::fdtables_call");

pub static ALL_COUNTERS: &[&Counter] = &[&LIBC_CALL, &FDTABLES_CALL];
