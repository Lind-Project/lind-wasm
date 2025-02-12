#![allow(dead_code)]
#![allow(unused_variables)]

// ===== User and Group ID Constants =====
// Lind-specific default values
pub const DEFAULT_UID: u32 = 1000;  // Default user ID
pub const DEFAULT_GID: u32 = 1000;  // Default group ID

// ===== Resource Limits =====
// Source: include/uapi/asm-generic/resource.h
pub const SIGNAL_MAX: i32 = 64;     // Maximum number of signals

// File descriptor limits
pub const NOFILE_CUR: u64 = 1024;   // Soft limit for number of open files
pub const NOFILE_MAX: u64 = 4 * 1024; // Hard limit for number of open files

// Stack size limits
pub const STACK_CUR: u64 = 8192 * 1024;  // Soft limit for stack size (8MB)
pub const STACK_MAX: u64 = 1 << 32;      // Hard limit for stack size (4GB)

// Resource identifiers
pub const RLIMIT_STACK: u64 = 0;    // Limit type for stack size
pub const RLIMIT_NOFILE: u64 = 1;   // Limit type for number of files

// ===== Process Exit Status =====
// Source: <stdlib.h> and POSIX standard
pub const EXIT_SUCCESS: i32 = 0;     // Successful termination
pub const EXIT_FAILURE: i32 = 1;     // Unsuccessful termination

// ===== Signal Constants =====
// Source: include/uapi/asm-generic/signal.h
// Reference: https://man7.org/linux/man-pages/man7/signal.7.html
// Note: Signal numbers can vary by architecture. These are for x86/ARM.

// Terminal control signals
pub const SIGHUP: i32 = 1;          // Hangup
pub const SIGINT: i32 = 2;          // Interrupt (Ctrl+C)
pub const SIGQUIT: i32 = 3;         // Quit (Ctrl+\)
pub const SIGTERM: i32 = 15;        // Termination request
pub const SIGKILL: i32 = 9;         // Forcefully kill a proces
pub const SIGSTKFLT: i32 = 16;      // Stack fault (unused on most systems)

// Error signals
pub const SIGILL: i32 = 4;          // Illegal instruction
pub const SIGTRAP: i32 = 5;         // Trace/breakpoint trap
pub const SIGABRT: i32 = 6;         // Abort program
pub const SIGIOT: i32 = 6;          // Alias for SIGABRT
pub const SIGBUS: i32 = 7;          // Bus error (bad memory access)
pub const SIGFPE: i32 = 8;          // Floating point exception
pub const SIGSEGV: i32 = 11;        // Segmentation violation
pub const SIGSYS: i32 = 31;         // Bad system call
pub const SIGUNUSED: i32 = 31;      // Alias for SIGSYS

// User-defined signals
pub const SIGUSR1: i32 = 10;        // User-defined signal 1
pub const SIGUSR2: i32 = 12;        // User-defined signal 2

// Process control signals
pub const SIGCHLD: i32 = 17;        // Child stopped or terminated
pub const SIGCONT: i32 = 18;        // Continue if stopped
pub const SIGSTOP: i32 = 19;        // Stop process
pub const SIGTSTP: i32 = 20;        // Stop typed at terminal
pub const SIGTTIN: i32 = 21;        // Terminal input for background process
pub const SIGTTOU: i32 = 22;        // Terminal output for background process

// Resource limit signals
pub const SIGXCPU: i32 = 24;        // CPU time limit exceeded
pub const SIGXFSZ: i32 = 25;        // File size limit exceeded

// Alarm signals
pub const SIGALRM: i32 = 14;        // Timer signal from alarm(2)
pub const SIGVTALRM: i32 = 26;      // Virtual timer expired
pub const SIGPROF: i32 = 27;        // Profiling timer expired

// I/O signals
pub const SIGPIPE: i32 = 13;        // Broken pipe
pub const SIGURG: i32 = 23;         // Urgent condition on socket
pub const SIGWINCH: i32 = 28;       // Window resize signal
pub const SIGIO: i32 = 29;          // I/O now possible
pub const SIGPOLL: i32 = 29;        // Pollable event (same as SIGIO)
pub const SIGPWR: i32 = 30;         // Power failure

pub const SIG_MAX: i32 = 32;        // maximum value of signal numbers

// Signal actions
pub const SIG_BLOCK: i32 = 0;       // Block signals in signal mask
pub const SIG_UNBLOCK: i32 = 1;     // Unblock signals in signal mask
pub const SIG_SETMASK: i32 = 2;     // Set the signal mask

// Signal flags
pub const SA_NOCLDSTOP: u32 = 0x00000001;       // Don't send SIGCHLD when children stop
pub const SA_NOCLDWAIT: u32 = 0x00000002;       // Don't create zombie on child death
pub const SA_SIGINFO: u32 = 0x00000004;         // Signal handler with SA_SIGINFO args
pub const SA_UNSUPPORTED: u32 = 0x00000400;     // Unsupported
pub const SA_EXPOSE_TAGBITS: u32 = 0x00000800;  // exposes an architecture-defined set of tag bits in siginfo.si_addr
pub const SA_ONSTACK: u32 = 0x08000000;         // Take signal on signal stack
pub const SA_RESTART: u32 = 0x10000000;         // Restart syscall on signal return
pub const SA_NODEFER: u32 = 0x40000000;         // Don't automatically block the signal when its handler is being executed
pub const SA_RESETHAND: u32 = 0x80000000;       // Reset to SIG_DFL on entry to handler

// Special Signal Handlers
pub const SIG_ERR: i32 = -1;        // Error return
pub const SIG_DFL: i32 = 0;         // Default action
pub const SIG_IGN: i32 = 1;         // Ignore signal

// Timer types
pub const ITIMER_REAL: i32 = 0;     // Real-time timer
