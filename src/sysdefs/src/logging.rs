//! Logging and debugging releated functions

//! This is a wrapper around rust's panic macro

pub fn lind_debug_panic(arg: &str) {
    panic!("{}", arg);
}
