//! Handler table implementation.
//!
//! This module provides the global `HANDLERTABLE` implementation, which
//! used in threei.rs.
//!
//! Two alternative backends are supported:
//! - `hashmap` (default, `Mutex<HashMap<..>>`) in `hashmap_impl.rs`
//! - `dashmap` (optional, concurrent `DashMap<..>`) in `dashmap_impl.rs`
//!
//! The dual implementation exists primarily so that future benchmarking
//! and performance analysis can easily compare `hashmap` vs `dashmap`
//! under different workloads. 
//!
//! See README.md in the project directory for usage details.
#[cfg(feature = "hashmap")]
pub mod hashmap_impl;

#[cfg(feature = "dashmap")]
pub mod dashmap_impl;

#[cfg(feature = "hashmap")]
pub use crate::handler_table::hashmap_impl::*;

#[cfg(feature = "dashmap")]
pub use crate::handler_table::dashmap_impl::*;
