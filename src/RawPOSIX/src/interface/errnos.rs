#![allow(dead_code)]
use crate::interface;

use std::sync::OnceLock;

pub static VERBOSE: OnceLock<isize> = OnceLock::new();

//A macro which takes the enum and adds to it a try_from trait which can convert values back to
//enum variants
macro_rules! reversible_enum {
    ($(#[$settings: meta])* $visibility: vis enum $enumname:ident {
        $($valuename: ident = $value: expr,)*
    }) => {
        $(#[$settings])*
        $visibility enum $enumname {
            $($valuename = $value,)*
        }

        impl $enumname {
            $visibility fn from_discriminant(v: i32) -> Result<Self, ()> {
                match v {
                    $($value => Ok($enumname::$valuename),)*
                    _ => Err(()),
                }
            }
        }
    }
}

reversible_enum! {
    #[derive(Debug, PartialEq, Eq)]
    #[repr(i32)]
    pub enum Errno {
        EPERM = 1,	// Operation not permitted
        ENOENT = 2, // No such file or directory
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

pub fn handle_errno(e: i32, syscall: &str) -> i32 {
    match e {
        // EPERM = 1,	// Operation not permitted
        1 => syscall_error(Errno::EPERM, syscall, "Operation not permitted"),
        // ENOENT = 2, // No such file or directory
        2 => syscall_error(Errno::ENOENT, syscall, "No such file or directory"),
        // ESRCH = 3,	// No such process
        3 => syscall_error(Errno::ESRCH, syscall, "No such process"),
        // EINTR = 4,	// Interrupted system call
        4 => syscall_error(Errno::EINTR, syscall, "Interrupted system call"),
        // EIO = 5,	// I/O error
        5 => syscall_error(Errno::EIO, syscall, "I/O error"),
        // ENXIO = 6,	// No such device or address
        6 => syscall_error(Errno::ENXIO, syscall, "No such device or address"),
        // EBIG = 7,	// Argument list too long
        7 => syscall_error(Errno::EBIG, syscall, "Argument list too long"),
        // ENOEXEC = 8,	// Exec format error
        8 => syscall_error(Errno::ENOEXEC, syscall, "Exec format error"),
        // EBADF = 9,	// Bad file number
        9 => syscall_error(Errno::EBADF, syscall, "Bad file number"),
        // ECHILD = 10,	// No child processes
        10 => syscall_error(Errno::ECHILD, syscall, "No child processes"),
        // EAGAIN = 11,	// Try again
        11 => syscall_error(Errno::EAGAIN, syscall, "Try again"),
        // ENOMEM = 12,	// c
        12 => syscall_error(Errno::ENOMEM, syscall, "Try again"),
        // EACCES = 13,	// Permission denied
        13 => syscall_error(Errno::EACCES, syscall, "Permission denied"),
        // EFAULT = 14,	// Bad address
        14 => syscall_error(Errno::EFAULT, syscall, "Bad address"),
        // ENOTBLK = 15,	// Block device required
        15 => syscall_error(Errno::ENOTBLK, syscall, "Block device required"),
        // EBUSY = 16,	// Device or resource busy
        16 => syscall_error(Errno::EBUSY, syscall, "Device or resource busy"),
        // EEXIST = 17,	// File exists
        17 => syscall_error(Errno::EEXIST, syscall, "File exists"),
        // EXDEV = 18,	// Cross-device link
        18 => syscall_error(Errno::EXDEV, syscall, "Cross-device link"),
        // ENODEV = 19,	// No such device
        19 => syscall_error(Errno::ENODEV, syscall, "No such device"),
        // ENOTDIR = 20,	// Not a directory
        20 => syscall_error(Errno::ENOTDIR, syscall, "Not a directory"),
        // EISDIR = 21,	// Is a directory
        21 => syscall_error(Errno::EISDIR, syscall, "Is a directory"),
        // EINVAL = 22,	// Invalid argument
        22 => syscall_error(Errno::EINVAL, syscall, "Invalid argument"),
        // ENFILE = 23,	// File table overflow
        23 => syscall_error(Errno::ENFILE, syscall, "File table overflow"),
        // EMFILE = 24,	// Too many open files
        24 => syscall_error(Errno::EMFILE, syscall, "Too many open files"),
        // ENOTTY = 25,	// Not a typewriter
        25 => syscall_error(Errno::ENOTTY, syscall, "Not a typewriter"),
        // ETXTBSY = 26,	// Text file busy
        26 => syscall_error(Errno::ETXTBSY, syscall, "Text file busy"),
        // EFBIG = 27,	// File too large
        27 => syscall_error(Errno::EFBIG, syscall, "File too large"),
        // ENOSPC = 28,	// No space left on device
        28 => syscall_error(Errno::ENOSPC, syscall, "No space left on device"),
        // ESPIPE = 29,	// Illegal seek
        29 => syscall_error(Errno::ESPIPE, syscall, "Illegal seek"),
        // EROFS = 30,	// Read-only file system
        30 => syscall_error(Errno::EROFS, syscall, "Read-only file system"),
        // EMLINK = 31,	// Too many links
        31 => syscall_error(Errno::EMLINK, syscall, "Too many links"),
        // EPIPE = 32,	// Broken pipe
        32 => syscall_error(Errno::EPIPE, syscall, "Broken pipe"),
        // EDOM = 33,	// Math argument out of domain of func
        33 => syscall_error(Errno::EDOM, syscall, "Math argument out of domain of func"),
        // ERANGE = 34,	// Math result not representable
        34 => syscall_error(Errno::ERANGE, syscall, "Math result not representable"),
        // EDEADLK = 35,	// Resource deadlock would occur
        35 => syscall_error(Errno::EDEADLK, syscall, "Resource deadlock would occur"),
        // ENAMETOOLONG = 36,	// File name too long
        36 => syscall_error(Errno::ENAMETOOLONG, syscall, "File name too long"),
        // ENOLCK = 37,  // No record locks available
        37 => syscall_error(Errno::ENOLCK, syscall, "No record locks available"),
        // ENOSYS = 38,	// Function not implemented
        38 => syscall_error(Errno::ENOSYS, syscall, "Function not implemented"),
        // ENOTEMPTY = 39,	// Directory not empty
        39 => syscall_error(Errno::ENOTEMPTY, syscall, "Directory not empty"),
        // ELOOP = 40,	// Too many symbolic links encountered
        40 => syscall_error(Errno::ELOOP, syscall, "Too many symbolic links encountered"),
        // // EWOULDBLOCK = 11, // Operation would block, returns EAGAIN
        // ENOMSG = 42,	// No message of desired type
        42 => syscall_error(Errno::ENOMSG, syscall, "No message of desired type"),
        // EIDRM = 43,	// Identifier removed
        43 => syscall_error(Errno::EIDRM, syscall, "Identifier removed"),
        // ECHRNG = 44,	// Channel number out of range
        44 => syscall_error(Errno::ECHRNG, syscall, "Channel number out of range"),
        // EL2NSYNC = 45,	// Level not synchronized
        45 => syscall_error(Errno::EL2NSYNC, syscall, "Level not synchronized"),
        // EL3HLT = 46,	// Level halted
        46 => syscall_error(Errno::EL3HLT, syscall, "Level halted"),
        // EL3RST = 47,	// Level reset
        47 => syscall_error(Errno::EL3RST, syscall, "Level reset"),
        // ELNRNG = 48,	// Link number out of range
        48 => syscall_error(Errno::ELNRNG, syscall, "Link number out of range"),
        // EUNATCH = 49,	// Protocol driver not attached
        49 => syscall_error(Errno::EUNATCH, syscall, "Protocol driver not attached"),
        // ENOCSI = 50,	// No CSI structure available
        50 => syscall_error(Errno::ENOCSI, syscall, "No CSI structure available"),
        // EL2HLT = 51,	// Level halted
        51 => syscall_error(Errno::EL2HLT, syscall, "Level halted"),
        // EBADE = 52,	// Invalid exchange
        52 => syscall_error(Errno::EBADE, syscall, "Invalid exchange"),
        // EBADR = 53,	// Invalid request descriptor
        53 => syscall_error(Errno::EBADR, syscall, "Invalid request descriptor"),
        // EXFULL = 54,	// Exchange full
        54 => syscall_error(Errno::EXFULL, syscall, "Exchange full"),
        // ENOANO = 55,	// No anode
        55 => syscall_error(Errno::ENOANO, syscall, "No anode"),
        // EBADRQC = 56,	// Invalid request code
        56 => syscall_error(Errno::EBADRQC, syscall, "Invalid request code"),
        // EBADSLT = 57,	// Invalid slot
        57 => syscall_error(Errno::EBADSLT, syscall, "Invalid slot"),
        // EBFONT = 59,	// Bad font file format
        59 => syscall_error(Errno::EBFONT, syscall, "Bad font file format"),
        // ENOSTR = 60,	// Device not a stream
        60 => syscall_error(Errno::ENOSTR, syscall, "Device not a stream"),
        // ENODATA = 61,	// No data available
        61 => syscall_error(Errno::ENODATA, syscall, "No data available"),
        // ETIME = 62,	// Timer expired
        62 => syscall_error(Errno::ETIME, syscall, "Timer expired"),
        // ENOSR = 63,	// Out of streams resources
        63 => syscall_error(Errno::ENOSR, syscall, "Out of streams resources"),
        // ENONET = 64,	// Machine is not on the network
        64 => syscall_error(Errno::ENONET, syscall, "Machine is not on the network"),
        // ENOPKG = 65,	// Package not installed
        65 => syscall_error(Errno::ENOPKG, syscall, "Package not installed"),
        // EREMOTE = 66,	// Object is remote
        66 => syscall_error(Errno::EREMOTE, syscall, "Object is remote"),
        // ENOLINK = 67,	// Link has been severed
        67 => syscall_error(Errno::ENOLINK, syscall, "Link has been severed"),
        // EADV = 68,	// Advertise error
        68 => syscall_error(Errno::EADV, syscall, "Advertise error"),
        // ESRMNT = 69,	// Srmount error
        69 => syscall_error(Errno::ESRMNT, syscall, "Srmount error"),
        // ECOMM = 70,	// Communication error on send
        70 => syscall_error(Errno::ECOMM, syscall, "Communication error on send"),
        // EPROTO = 71,	// Protocol error
        71 => syscall_error(Errno::EPROTO, syscall, "Protocol error"),
        // EMULTIHOP = 72,	// Multihop attempted
        72 => syscall_error(Errno::EMULTIHOP, syscall, "Multihop attempted"),
        // EDOTDOT = 73,	// RFS specific error
        73 => syscall_error(Errno::EDOTDOT, syscall, "RFS specific error"),
        // EBADMSG = 74,	// Not a data message
        74 => syscall_error(Errno::EBADMSG, syscall, "Not a data message"),
        // EOVERFLOW = 75,	// Value too large for defined data type
        75 => syscall_error(
            Errno::EOVERFLOW,
            syscall,
            "Value too large for defined data type",
        ),
        // ENOTUNIQ = 76,	// Name not unique on network
        76 => syscall_error(Errno::ENOTUNIQ, syscall, "Name not unique on network"),
        // EBADFD = 77,	// File descriptor in bad state
        77 => syscall_error(Errno::EBADFD, syscall, "File descriptor in bad state"),
        // EREMCHG = 78,	// Remote address changed
        78 => syscall_error(Errno::EREMCHG, syscall, "Remote address changed"),
        // ELIBACC = 79,	// Can not access a needed shared library
        79 => syscall_error(
            Errno::ELIBACC,
            syscall,
            "Can not access a needed shared library",
        ),
        // ELIBBAD = 80,	// Accessing a corrupted shared library
        80 => syscall_error(
            Errno::ELIBBAD,
            syscall,
            "Accessing a corrupted shared library",
        ),
        // ELIBSCN = 81,	// .lib section in a.out corrupted
        81 => syscall_error(Errno::ELIBSCN, syscall, ".lib section in a.out corrupted"),
        // ELIBMAX = 82,	// Attempting to link in too many shared libraries
        82 => syscall_error(
            Errno::ELIBMAX,
            syscall,
            "Attempting to link in too many shared libraries",
        ),
        // ELIBEXEC = 83,	// Cannot exec a shared library directly
        83 => syscall_error(
            Errno::ELIBEXEC,
            syscall,
            "Cannot exec a shared library directly",
        ),
        // EILSEQ = 84,	// Illegal byte sequence
        84 => syscall_error(Errno::EILSEQ, syscall, "Illegal byte sequence"),
        // ERESTART = 85,	// Interrupted system call should be restarted
        85 => syscall_error(
            Errno::ERESTART,
            syscall,
            "Interrupted system call should be restarted",
        ),
        // ESTRPIPE = 86,	// Streams pipe error
        86 => syscall_error(Errno::ESTRPIPE, syscall, "Streams pipe error"),
        // EUSERS = 87,	// Too many users
        87 => syscall_error(Errno::EUSERS, syscall, "Too many users"),
        // ENOTSOCK = 88,	// Socket operation on non-socket
        88 => syscall_error(Errno::ENOTSOCK, syscall, "Socket operation on non-socket"),
        // EDESTADDRREQ = 89,	// Destination address required
        89 => syscall_error(Errno::EDESTADDRREQ, syscall, "Destination address required"),
        // EMSGSIZE = 90,	// Message too long
        90 => syscall_error(Errno::EMSGSIZE, syscall, "Message too long"),
        // EPROTOTYPE = 91,	// Protocol wrong type for socket
        91 => syscall_error(Errno::EPROTOTYPE, syscall, "Protocol wrong type for socket"),
        // ENOPROTOOPT = 92,	// Protocol not available
        92 => syscall_error(Errno::ENOPROTOOPT, syscall, "Protocol not available"),
        // EPROTONOSUPPORT = 93,	// Protocol not supported
        93 => syscall_error(Errno::EPROTONOSUPPORT, syscall, "Protocol not supported"),
        // ESOCKTNOSUPPORT = 94,	// Socket type not supported
        94 => syscall_error(Errno::ESOCKTNOSUPPORT, syscall, "Socket type not supported"),
        // EOPNOTSUPP = 95,	// Operation not supported on transport endpoint
        95 => syscall_error(
            Errno::EOPNOTSUPP,
            syscall,
            "Operation not supported on transport endpoint",
        ),
        // EPFNOSUPPORT = 96,	// Protocol family not supported
        96 => syscall_error(
            Errno::EPFNOSUPPORT,
            syscall,
            "Protocol family not supported",
        ),
        // EAFNOSUPPORT = 97,	// Address family not supported by protocol
        97 => syscall_error(
            Errno::EAFNOSUPPORT,
            syscall,
            "Address family not supported by protocol",
        ),
        // EADDRINUSE = 98,	// Address already in use
        98 => syscall_error(Errno::EADDRINUSE, syscall, "Address already in use"),
        // EADDRNOTAVAIL = 99,	// Cannot assign requested address
        99 => syscall_error(
            Errno::EADDRNOTAVAIL,
            syscall,
            "Cannot assign requested address",
        ),
        // ENETDOWN = 100,	// Network is down
        100 => syscall_error(Errno::ENETDOWN, syscall, "Network is down"),
        // ENETUNREACH = 101,	// Network is unreachable
        101 => syscall_error(Errno::ENETUNREACH, syscall, "Network is unreachable"),
        // ENETRESET = 102,	// Network dropped connection because of reset
        102 => syscall_error(
            Errno::ENETRESET,
            syscall,
            "Network dropped connection because of reset",
        ),
        // ECONNABORTED = 103,	// Software caused connection abort
        103 => syscall_error(
            Errno::ECONNABORTED,
            syscall,
            "Software caused connection abort",
        ),
        // ECONNRESET = 104,	// Connection reset by peer
        104 => syscall_error(Errno::ECONNRESET, syscall, "Connection reset by peer"),
        // ENOBUFS = 105,	// No buffer space available
        105 => syscall_error(Errno::ENOBUFS, syscall, "No buffer space available"),
        // EISCONN = 106,	// Transport endpoint is already connected
        106 => syscall_error(
            Errno::EISCONN,
            syscall,
            "Transport endpoint is already connected",
        ),
        // ENOTCONN = 107,	// Transport endpoint is not connected
        107 => syscall_error(
            Errno::ENOTCONN,
            syscall,
            "Transport endpoint is not connected",
        ),
        // ESHUTDOWN = 108,	// Cannot send after transport endpoint shutdown
        108 => syscall_error(
            Errno::ESHUTDOWN,
            syscall,
            "Cannot send after transport endpoint shutdown",
        ),
        // ETOOMANYREFS = 109,	// Too many references cannot splice
        109 => syscall_error(
            Errno::ETOOMANYREFS,
            syscall,
            "Too many references cannot splice",
        ),
        // ETIMEDOUT = 110,	// Connection timed out
        110 => syscall_error(Errno::ETIMEDOUT, syscall, "Connection timed out"),
        // ECONNREFUSED = 111,	// Connection refused
        111 => syscall_error(Errno::ECONNREFUSED, syscall, "Connection refused"),
        // EHOSTDOWN = 112,	// Host is down
        112 => syscall_error(Errno::EHOSTDOWN, syscall, "Host is down"),
        // EHOSTUNREACH = 113,	// No route to host
        113 => syscall_error(Errno::EHOSTUNREACH, syscall, "No route to host"),
        // EALREADY = 114,	// Operation already in progress
        114 => syscall_error(Errno::EALREADY, syscall, "Operation already in progress"),
        // EINPROGRESS = 115,	// Operation now in progress
        115 => syscall_error(Errno::EINPROGRESS, syscall, "Operation now in progress"),
        // ESTALE = 116,	// Stale NFS file handle
        116 => syscall_error(Errno::ESTALE, syscall, "Stale NFS file handle"),
        // EUCLEAN = 117,	// Structure needs cleaning
        117 => syscall_error(Errno::EUCLEAN, syscall, "Structure needs cleaning"),
        // ENOTNAM = 118,	// Not a XENIX named type file
        118 => syscall_error(Errno::ENOTNAM, syscall, "Not a XENIX named type file"),
        // ENAVAIL = 119,	// No XENIX semaphores available
        119 => syscall_error(Errno::ENAVAIL, syscall, "No XENIX semaphores available"),
        // EISNAM = 120,	// Is a named type file
        120 => syscall_error(Errno::EISNAM, syscall, "Is a named type file"),
        // EREMOTEIO = 121,	// Remote I/O error
        121 => syscall_error(Errno::EREMOTEIO, syscall, "Remote I/O error"),
        // EDQUOT = 122,	// Quota exceeded
        122 => syscall_error(Errno::EDQUOT, syscall, "Quota exceeded"),
        // ENOMEDIUM = 123,	// No medium found
        123 => syscall_error(Errno::ENOMEDIUM, syscall, "No medium found"),
        // EMEDIUMTYPE = 124,	// Wrong medium type
        124 => syscall_error(Errno::EMEDIUMTYPE, syscall, "Wrong medium type"),
        // ECANCELED = 125,	// Operation Canceled
        125 => syscall_error(Errno::ECANCELED, syscall, "Operation Canceled"),
        // ENOKEY = 126,	// Required key not available
        126 => syscall_error(Errno::ENOKEY, syscall, "Required key not available"),
        // EKEYEXPIRED = 127,	// Key has expired
        127 => syscall_error(Errno::EKEYEXPIRED, syscall, "Key has expired"),
        // EKEYREVOKED = 128,	// Key has been revoked
        128 => syscall_error(Errno::EKEYREVOKED, syscall, "Key has been revoked"),
        // EKEYREJECTED = 129,	// Key was rejected by service  for robust mutexes
        129 => syscall_error(
            Errno::EKEYREJECTED,
            syscall,
            "Key was rejected by service  for robust mutexes",
        ),
        // EOWNERDEAD = 130,	// Owner died
        130 => syscall_error(Errno::EOWNERDEAD, syscall, "Owner died"),
        // ENOTRECOVERABLE = 131, // State not recoverable
        131 => syscall_error(Errno::ENOTRECOVERABLE, syscall, "State not recoverable"),
        _ => syscall_error(Errno::EINVAL, syscall, "Invalid error code"),
    }
}

pub fn syscall_error(e: Errno, syscall: &str, message: &str) -> i32 {
    if *VERBOSE.get().unwrap() > 0 {
        let msg = format!("Error in syscall: {} - {:?}: {}", syscall, e, message);
        interface::log_to_stderr(&msg);
    }
    -(e as i32)
}
