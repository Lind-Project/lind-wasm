// System related constants
#![allow(dead_code)]
#![allow(unused_variables)]

use crate::interface;

// Define constants using static or const
// Imported into fs_calls file

//GID AND UID DEFAULT VALUES

pub const DEFAULT_UID: u32 = 1000;
pub const DEFAULT_GID: u32 = 1000;

// RESOURCE LIMITS

pub const SIGNAL_MAX: i32 = 64;

pub const NOFILE_CUR: u64 = 1024;
pub const NOFILE_MAX: u64 = 4 * 1024;

pub const STACK_CUR: u64 = 8192 * 1024;
pub const STACK_MAX: u64 = 1 << 32;

pub const RLIMIT_STACK: u64 = 0;
pub const RLIMIT_NOFILE: u64 = 1;

// Constants for exit_syscall status

pub const EXIT_SUCCESS: i32 = 0;
pub const EXIT_FAILURE: i32 = 1;

// Signal Table (x86/ARM)
// Based on https://man7.org/linux/man-pages/man7/signal.7.html
pub const SIGHUP: i32 = 1;
pub const SIGINT: i32 = 2;
pub const SIGQUIT: i32 = 3;
pub const SIGILL: i32 = 4;
pub const SIGTRAP: i32 = 5;
pub const SIGABRT: i32 = 6;
pub const SIGIOT: i32 = 6;
pub const SIGBUS: i32 = 7;
// pub const SIGEMT: i32
pub const SIGFPE: i32 = 8;
pub const SIGKILL: i32 = 9;
pub const SIGUSR1: i32 = 10;
pub const SIGSEGV: i32 = 11;
pub const SIGUSR2: i32 = 12;
pub const SIGPIPE: i32 = 13;
pub const SIGALRM: i32 = 14;
pub const SIGTERM: i32 = 15;
pub const SIGSTKFLT: i32 = 16;
pub const SIGCHLD: i32 = 17;
// pub const SIGCLD: i32
pub const SIGCONT: i32 = 18;
pub const SIGSTOP: i32 = 19;
pub const SIGTSTP: i32 = 20;
pub const SIGTTIN: i32 = 21;
pub const SIGTTOU: i32 = 22;
pub const SIGURG: i32 = 23;
pub const SIGXCPU: i32 = 24;
pub const SIGXFSZ: i32 = 25;
pub const SIGVTALRM: i32 = 26;
pub const SIGPROF: i32 = 27;
pub const SIGWINCH: i32 = 28;
pub const SIGIO: i32 = 29;
pub const SIGPOLL: i32 = 29;
pub const SIGPWR: i32 = 30;
// pub const SIGINFO: i32
// pub const SIGLOST: i32
pub const SIGSYS: i32 = 31;
pub const SIGUNUSED: i32 = 31;

pub const SIG_BLOCK: i32 = 0;
pub const SIG_UNBLOCK: i32 = 1;
pub const SIG_SETMASK: i32 = 2;
pub const ITIMER_REAL: i32 = 0;
