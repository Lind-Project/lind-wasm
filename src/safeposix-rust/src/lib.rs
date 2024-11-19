#![feature(lazy_cell)]
#![feature(rustc_private)] //for private crate imports for tests
#![feature(vec_into_raw_parts)]
#![feature(thread_local)]
#![allow(unused_imports)]

// interface and safeposix are public because otherwise there isn't a great
// way to 'use' them for benchmarking.
pub mod interface;
mod lib_fs_utils;
pub mod safeposix;
pub mod tests;
