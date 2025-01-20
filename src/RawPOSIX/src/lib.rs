#![feature(lazy_cell)]
#![feature(rustc_private)] //for private crate imports for tests
#![feature(vec_into_raw_parts)]
#![feature(thread_local)]
#![allow(unused_imports)]
#![feature(hash_extract_if)]

// interface and safeposix are public because otherwise there isn't a great
// way to 'use' them for benchmarking.
pub mod interface;
pub mod safeposix;
pub mod tests;
pub mod fdtables;
pub mod constants;
