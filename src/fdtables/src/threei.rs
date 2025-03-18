//! Common constants needed for interface communication.  Contains things
//! like errno values.
// Defining a few basic things about the 3i interface here...
// This needs enough information that we can make calls effectively and
// so needs errno, etc.

// Let's not have clippy warn for EAGAIN, etc.
#![allow(clippy::upper_case_acronyms)]
// Don't warn if all listed things (like errnos) are not used in code...
#![allow(dead_code)]

// Define some cageid constants that may be useful.  These are not valid for
// normal use as cageids

#[doc(hidden)]
pub const INVALID_CAGEID: u64 = 0xffff_ffff_ffff_fffe;

// Used for internal testing.  Not valid for a normal cageid...
#[doc(hidden)]
pub const TESTING_CAGEID: u64 = 0xffff_ffff_ffff_ffe0;
#[doc(hidden)]
pub const TESTING_CAGEID0: u64 = 0xffff_ffff_ffff_ffe0;
#[doc(hidden)]
pub const TESTING_CAGEID1: u64 = 0xffff_ffff_ffff_ffe1;
#[doc(hidden)]
pub const TESTING_CAGEID2: u64 = 0xffff_ffff_ffff_ffe2;
#[doc(hidden)]
pub const TESTING_CAGEID3: u64 = 0xffff_ffff_ffff_ffe3;
#[doc(hidden)]
pub const TESTING_CAGEID4: u64 = 0xffff_ffff_ffff_ffe4;
#[doc(hidden)]
pub const TESTING_CAGEID5: u64 = 0xffff_ffff_ffff_ffe5;
#[doc(hidden)]
pub const TESTING_CAGEID6: u64 = 0xffff_ffff_ffff_ffe6;
#[doc(hidden)]
pub const TESTING_CAGEID7: u64 = 0xffff_ffff_ffff_ffe7;
#[doc(hidden)]
pub const TESTING_CAGEID8: u64 = 0xffff_ffff_ffff_ffe8;
#[doc(hidden)]
pub const TESTING_CAGEID9: u64 = 0xffff_ffff_ffff_ffe9;
#[doc(hidden)]
pub const TESTING_CAGEID10: u64 = 0xffff_ffff_ffff_ffea;
#[doc(hidden)]
pub const TESTING_CAGEID11: u64 = 0xffff_ffff_ffff_ffeb;
#[doc(hidden)]
pub const TESTING_CAGEID12: u64 = 0xffff_ffff_ffff_ffec;
#[doc(hidden)]
pub const TESTING_CAGEID13: u64 = 0xffff_ffff_ffff_ffed;
#[doc(hidden)]
pub const TESTING_CAGEID14: u64 = 0xffff_ffff_ffff_ffee;
#[doc(hidden)]
pub const TESTING_CAGEID15: u64 = 0xffff_ffff_ffff_ffef;

#[doc(hidden)]
macro_rules! reversible_enum {
    ($(#[$settings: meta])* $visibility: vis enum $enumname:ident {
        $($valuename: ident = $value: expr,)*
    }) => {
        $(#[$settings])*
        #[doc(hidden)]
        $visibility enum $enumname {
            $($valuename = $value,)*
        }

        impl $enumname {
            #[doc(hidden)]
            $visibility fn from_discriminant(v: u64) -> Result<Self, ()> {
                match v {
                    $($value => Ok($enumname::$valuename),)*
                    _ => Err(()),
                }
            }
        }
    }
}

/// Return value for system calls...  Can be errno
pub type RetVal = u64;

// BUG: ? I don't understand this setup...
reversible_enum! {
    #[derive(Debug, PartialEq, Eq)]
    #[repr(u64)]
    /// Errno values for OS calls
    #[non_exhaustive] // I want to be able to update this later...
    pub enum Errno {
        EPERM = 1,	// Operation not permitted
        ENOENT = 2,     // No such file or directory
        ESRCH = 3,	// No such process
        EINTR = 4,	// Interrupted system call
        EIO = 5,	// I/O error
        ENXIO = 6,	// No such device or address
        EBIG = 7,	// Argument list too long
        ENOEXEC = 8,	// Exec format error
        EBADF = 9,	// Bad file number
        ECHILD = 10,	// No child processes
        EAGAIN = 11,	// Try again
        ENOMEM = 12,	// Out of memory
        EACCES = 13,	// Permission denied
        EFAULT = 14,	// Bad address
        ENOTBLK = 15,	// Block device required
        EBUSY = 16,	// Device or resource busy
        EEXIST = 17,	// File exists
        EXDEV = 18,	// Cross-device link
        ENODEV = 19,	// No such device
        ENOTDIR = 20,	// Not a directory
        EISDIR = 21,	// Is a directory
        EINVAL = 22,	// Invalid argument
        ENFILE = 23,	// File table overflow
        EMFILE = 24,	// Too many open files
        ENOTTY = 25,	// Not a typewriter
        ETXTBSY = 26,	// Text file busy
        EFBIG = 27,	// File too large
        ENOSPC = 28,	// No space left on device
        ESPIPE = 29,	// Illegal seek
        EROFS = 30,	// Read-only file system
        EMLINK = 31,	// Too many links
        EPIPE = 32,	// Broken pipe
        EDOM = 33,	// Math argument out of domain of func
        ERANGE = 34,	// Math result not representable
        EDEADLK = 35,	// Resource deadlock would occur
        ENAMETOOLONG = 36,	// File name too long
        ENOLCK = 37,  // No record locks available
        ENOSYS = 38,	// Function not implemented
        ENOTEMPTY = 39,	// Directory not empty
        ELOOP = 40,	// Too many symbolic links encountered
        // EWOULDBLOCK = 11, // Operation would block, returns EAGAIN
        ENOMSG = 42,	// No message of desired type
        EIDRM = 43,	// Identifier removed
        ECHRNG = 44,	// Channel number out of range
        EL2NSYNC = 45,	// Level  not synchronized
        EL3HLT = 46,	// Level  halted
        EL3RST = 47,	// Level  reset
        ELNRNG = 48,	// Link number out of range
        EUNATCH = 49,	// Protocol driver not attached
        ENOCSI = 50,	// No CSI structure available
        EL2HLT = 51,	// Level  halted
        EBADE = 52,	// Invalid exchange
        EBADR = 53,	// Invalid request descriptor
        EXFULL = 54,	// Exchange full
        ENOANO = 55,	// No anode
        EBADRQC = 56,	// Invalid request code
        EBADSLT = 57,	// Invalid slot
        EBFONT = 59,	// Bad font file format
        ENOSTR = 60,	// Device not a stream
        ENODATA = 61,	// No data available
        ETIME = 62,	// Timer expired
        ENOSR = 63,	// Out of streams resources
        ENONET = 64,	// Machine is not on the network
        ENOPKG = 65,	// Package not installed
        EREMOTE = 66,	// Object is remote
        ENOLINK = 67,	// Link has been severed
        EADV = 68,	// Advertise error
        ESRMNT = 69,	// Srmount error
        ECOMM = 70,	// Communication error on send
        EPROTO = 71,	// Protocol error
        EMULTIHOP = 72,	// Multihop attempted
        EDOTDOT = 73,	// RFS specific error
        EBADMSG = 74,	// Not a data message
        EOVERFLOW = 75,	// Value too large for defined data type
        ENOTUNIQ = 76,	// Name not unique on network
        EBADFD = 77,	// File descriptor in bad state
        EREMCHG = 78,	// Remote address changed
        ELIBACC = 79,	// Can not access a needed shared library
        ELIBBAD = 80,	// Accessing a corrupted shared library
        ELIBSCN = 81,	// .lib section in a.out corrupted
        ELIBMAX = 82,	// Attempting to link in too many shared libraries
        ELIBEXEC = 83,	// Cannot exec a shared library directly
        EILSEQ = 84,	// Illegal byte sequence
        ERESTART = 85,	// Interrupted system call should be restarted
        ESTRPIPE = 86,	// Streams pipe error
        EUSERS = 87,	// Too many users
        ENOTSOCK = 88,	// Socket operation on non-socket
        EDESTADDRREQ = 89,	// Destination address required
        EMSGSIZE = 90,	// Message too long
        EPROTOTYPE = 91,	// Protocol wrong type for socket
        ENOPROTOOPT = 92,	// Protocol not available
        EPROTONOSUPPORT = 93,	// Protocol not supported
        ESOCKTNOSUPPORT = 94,	// Socket type not supported
        EOPNOTSUPP = 95,	// Operation not supported on transport endpoint
        EPFNOSUPPORT = 96,	// Protocol family not supported
        EAFNOSUPPORT = 97,	// Address family not supported by protocol
        EADDRINUSE = 98,	// Address already in use
        EADDRNOTAVAIL = 99,	// Cannot assign requested address
        ENETDOWN = 100,	// Network is down
        ENETUNREACH = 101,	// Network is unreachable
        ENETRESET = 102,	// Network dropped connection because of reset
        ECONNABORTED = 103,	// Software caused connection abort
        ECONNRESET = 104,	// Connection reset by peer
        ENOBUFS = 105,	// No buffer space available
        EISCONN = 106,	// Transport endpoint is already connected
        ENOTCONN = 107,	// Transport endpoint is not connected
        ESHUTDOWN = 108,	// Cannot send after transport endpoint shutdown
        ETOOMANYREFS = 109,	// Too many references cannot splice
        ETIMEDOUT = 110,	// Connection timed out
        ECONNREFUSED = 111,	// Connection refused
        EHOSTDOWN = 112,	// Host is down
        EHOSTUNREACH = 113,	// No route to host
        EALREADY = 114,	// Operation already in progress
        EINPROGRESS = 115,	// Operation now in progress
        ESTALE = 116,	// Stale NFS file handle
        EUCLEAN = 117,	// Structure needs cleaning
        ENOTNAM = 118,	// Not a XENIX named type file
        ENAVAIL = 119,	// No XENIX semaphores available
        EISNAM = 120,	// Is a named type file
        EREMOTEIO = 121,	// Remote I/O error
        EDQUOT = 122,	// Quota exceeded
        ENOMEDIUM = 123,	// No medium found
        EMEDIUMTYPE = 124,	// Wrong medium type
        ECANCELED = 125,	// Operation Canceled
        ENOKEY = 126,	// Required key not available
        EKEYEXPIRED = 127,	// Key has expired
        EKEYREVOKED = 128,	// Key has been revoked
        EKEYREJECTED = 129,	// Key was rejected by service  for robust mutexes
        EOWNERDEAD = 130,	// Owner died
        ENOTRECOVERABLE = 131, // State not recoverable
    }
}
