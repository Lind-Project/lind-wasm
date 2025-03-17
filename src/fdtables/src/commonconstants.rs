// This file exists to make it easier to vary a single file of constants
// instead of editing each implementation...

/// Per-process maximum number of fds...
pub const FD_PER_PROCESS_MAX: u64 = 1024;

// /// Use this to indicate there isn't a real fd backing an item
//pub const NO_REAL_FD: u64 = 0xff_abcd_ef01;

// /// Use to indicate this is an EPOLLFD
// pub const EPOLLFD: u64 = 0xff_abcd_ef02;

/// All FDKIND values defined by the user must be below this value.
pub const FDT_KINDMAX: u32 = 0xff00_0000;

/// Use this to indicate that a FD is invalid... Usually an error will be
/// returned instead, but this is needed for rare cases like poll.
pub const FDT_INVALID_FD: u32 = 0xff00_0001;

/// Use to indicate this is an EPOLLFD (an internal kind of fd)
pub const FDT_KINDEPOLL: u32 = 0xff00_0002;

// These are the values we look up with at the end...
#[doc = include_str!("../docs/fdtableentry.md")]
#[derive(Clone, Copy, Hash, Debug, PartialEq, Eq)]
/// This is a table entry, looked up by virtual fd.
pub struct FDTableEntry {
    /// This is the kind of fd which it is.  These are user defined values
    /// so they can track what this means.  Appropriate for storing information
    /// which is global to all virtual fds that track this.
    /// values over FDKINDMAX are reserved.
    pub fdkind: u32,
    /// underlying fd (could be the real, kernel fd below us or could be
    /// some indicator of what table entry a virtual fd has.  It is up to
    /// the implementer to decide how to use this.
    pub underfd: u64,
    /// Should I close this on exec?  Needed so fdtabls can implement
    /// [/`empty_fds_for_exec`]
    pub should_cloexec: bool,
    /// Used to store fd specific extra information, such as flags or similar
    /// which may differ for different 'dup'ed copies of a fd.   Whatever
    /// the user desires may be placed here.
    pub perfdinfo: u64,
}

#[allow(non_snake_case)]
/// A function used when registering close handlers which does nothing...
/// It is the default if no close handlers are defined
pub const fn NULL_FUNC(_: FDTableEntry, _: u64) {}

// BUG / TODO: Use this in some sane way...
#[allow(dead_code)]
/// Global maximum number of fds... (checks may not be implemented)
pub const TOTAL_FD_MAX: u64 = 4096;

// replicating these constants here so this can compile on systems other than
// Linux...  Copied from Rust's libc.
/// copied from libc
pub const EPOLL_CTL_ADD: i32 = 1;
/// copied from libc
pub const EPOLL_CTL_MOD: i32 = 2;
/// copied from libc
pub const EPOLL_CTL_DEL: i32 = 3;

#[allow(non_camel_case_types)]
/// i32 copied from libc.  used in EPOLL event flags even though events are u32
pub type c_int = i32;

/// copied from libc
pub const EPOLLIN: c_int = 0x1;
/// copied from libc
pub const EPOLLPRI: c_int = 0x2;
/// copied from libc
pub const EPOLLOUT: c_int = 0x4;
/// copied from libc
pub const EPOLLERR: c_int = 0x8;
/// copied from libc
pub const EPOLLHUP: c_int = 0x10;
/// copied from libc
pub const EPOLLRDNORM: c_int = 0x40;
/// copied from libc
pub const EPOLLRDBAND: c_int = 0x80;
/// copied from libc
pub const EPOLLWRNORM: c_int = 0x100;
/// copied from libc
pub const EPOLLWRBAND: c_int = 0x200;
/// copied from libc
pub const EPOLLMSG: c_int = 0x400;
/// copied from libc
pub const EPOLLRDHUP: c_int = 0x2000;
/// copied from libc
pub const EPOLLEXCLUSIVE: c_int = 0x1000_0000;
/// copied from libc
pub const EPOLLWAKEUP: c_int = 0x2000_0000;
/// copied from libc
pub const EPOLLONESHOT: c_int = 0x4000_0000;
// Turning this on here because we copied from Rust's libc and I assume they
// intended this...
#[allow(overflowing_literals)]
/// copied from libc
pub const EPOLLET: c_int = 0x8000_0000;

// use libc::epoll_event;
// Note, I'm not using libc's version because this isn't defined on Windows
// or Mac.  Hence, I can't compile, etc. on those systems.  Of course any
// system actually running epoll, will need to be on Mac, but that doesn't mean
// we can't parse those calls.
#[allow(non_camel_case_types)]
#[derive(Clone, Debug)]
/// matches libc in Rust.  Copied exactly.
pub struct epoll_event {
    /// copied from libc.  Event types to look at.
    pub events: u32, // So weird that this is a u32, while the constants
    // defined to work with it are i32s...
    /// copied from libc.  Not used.
    pub u64: u64,
}
