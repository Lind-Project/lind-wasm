use lind_perf::Counter;

pub static CALL_GRATE_FUNC: Counter = Counter::new("threei::_call_grate_func");
pub static MAKE_SYSCALL: Counter = Counter::new("threei::make_syscall");
pub static RAWPOSIX_DISPATCH: Counter = Counter::new("threei::rawposix_dispatch");

pub static ALL_COUNTERS: &[&Counter] = &[&CALL_GRATE_FUNC, &MAKE_SYSCALL, &RAWPOSIX_DISPATCH];
