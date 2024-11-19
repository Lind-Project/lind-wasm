#![allow(dead_code)] //suppress warning for these functions not being used in targets other than the tests

mod fs_tests;
mod ipc_tests;
mod networking_tests;
use rand::Rng;
use std::net::{TcpListener, UdpSocket};

use crate::interface;
use crate::safeposix::{cage::*, filesystem::*};

#[cfg(test)]
mod main_tests {
    use crate::tests::fs_tests::fs_tests::test_fs;
    use crate::tests::ipc_tests::ipc_tests::test_ipc;
    use crate::tests::networking_tests::net_tests::net_tests;

    use crate::interface;
    use crate::safeposix::{cage::*, dispatcher::*, filesystem::*};

    use std::process::Command;

    #[test]
    pub fn tests() {
        interface::RUSTPOSIX_TESTSUITE.store(true, interface::RustAtomicOrdering::Relaxed);

        lindrustinit(0);
        {
            let cage = interface::cagetable_getref(1);
            crate::lib_fs_utils::lind_deltree(&cage, "/");
            assert_eq!(cage.mkdir_syscall("/dev", S_IRWXA), 0);
            assert_eq!(
                cage.mknod_syscall(
                    "/dev/null",
                    S_IFCHR as u32 | 0o777,
                    makedev(&DevNo { major: 1, minor: 3 })
                ),
                0
            );
            assert_eq!(
                cage.mknod_syscall(
                    "/dev/zero",
                    S_IFCHR as u32 | 0o777,
                    makedev(&DevNo { major: 1, minor: 5 })
                ),
                0
            );
            assert_eq!(
                cage.mknod_syscall(
                    "/dev/urandom",
                    S_IFCHR as u32 | 0o777,
                    makedev(&DevNo { major: 1, minor: 9 })
                ),
                0
            );
            assert_eq!(
                cage.mknod_syscall(
                    "/dev/random",
                    S_IFCHR as u32 | 0o777,
                    makedev(&DevNo { major: 1, minor: 8 })
                ),
                0
            );
            assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        }
        lindrustfinalize();

        println!("FS TESTS");
        test_fs();

        println!("NET TESTS");
        net_tests();

        println!("IPC TESTS");
        test_ipc();
    }
}

pub fn str2cbuf(ruststr: &str) -> *mut u8 {
    let cbuflenexpected = ruststr.len();
    let (ptr, len, _) = ruststr.to_string().into_raw_parts();
    assert_eq!(len, cbuflenexpected);
    return ptr;
}

pub fn sizecbuf<'a>(size: usize) -> Box<[u8]> {
    let v = vec![0u8; size];
    v.into_boxed_slice()
    //buf.as_mut_ptr() as *mut u8
}

pub fn cbuf2str(buf: &[u8]) -> &str {
    std::str::from_utf8(buf).unwrap()
}

// The RustPOSIX test suite avoids conflicts caused by repeatedly binding to the same ports by generating a random port number within the valid range (49152-65535) for each test run. This eliminates the need for waiting between tests.

fn is_port_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok() &&
    UdpSocket::bind(("127.0.0.1", port)).is_ok()
}

pub fn generate_random_port() -> u16 {
    for port in 49152..65535 {
        if is_port_available(port) {
            return port;
        }
    }
    panic!("No available ports found");
}
