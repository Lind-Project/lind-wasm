mod timers;

#[cfg(not(feature = "enabled"))]
mod disabled;
#[cfg(feature = "enabled")]
mod enabled;

pub use timers::*;

#[cfg(not(feature = "enabled"))]
pub use disabled::*;
#[cfg(feature = "enabled")]
pub use enabled::*;

// Exported runtime flag used by callers (for example lind-boot CLI handling)
// to decide whether a requested perf mode can actually run.
#[cfg(not(feature = "enabled"))]
pub static ENABLED: bool = false;
#[cfg(feature = "enabled")]
pub static ENABLED: bool = true;

#[macro_export]
macro_rules! get_timer {
    // Always available macro. In disabled builds, `get_timer()` returns a
    // no-op scope object from `disabled::Counter`.
    ($counter:path) => {{ $counter.get_timer() }};
}
