use rawposix::SYSCALL_TABLE;
use std::ffi::CStr;
use std::mem::{size_of, MaybeUninit};
use sysdefs::constants::err_const::VERBOSE;
use sysdefs::constants::lind_platform_const::{UNUSED_ARG, UNUSED_ID};
use sysdefs::constants::syscall_const::UNAME_SYSCALL;

#[test]
fn uname_dispatch_writes_kernel_layout_and_returns_errno() {
    VERBOSE.set(0).unwrap();
    let handler = SYSCALL_TABLE
        .iter()
        .find(|(number, _)| *number == UNAME_SYSCALL as u64)
        .expect("uname must be registered for native loader startup")
        .1;
    let call = |buffer| {
        handler(
            1, buffer, 1, UNUSED_ARG, UNUSED_ID, UNUSED_ARG, UNUSED_ID, UNUSED_ARG, UNUSED_ID,
            UNUSED_ARG, UNUSED_ID, UNUSED_ARG, UNUSED_ID,
        )
    };
    #[repr(C)]
    struct Buffer {
        before: u64,
        name: MaybeUninit<libc::utsname>,
        after: u64,
    }
    let mut buffer = Buffer {
        before: 0xA5A5_A5A5_A5A5_A5A5,
        name: MaybeUninit::uninit(),
        after: 0xA5A5_A5A5_A5A5_A5A5,
    };
    assert_eq!(size_of::<libc::utsname>(), 6 * 65);
    assert_eq!(call(buffer.name.as_mut_ptr() as u64), 0);
    let name = unsafe { buffer.name.assume_init() };
    assert_eq!(
        unsafe { CStr::from_ptr(name.sysname.as_ptr()) }.to_bytes(),
        b"Linux"
    );
    assert!(!unsafe { CStr::from_ptr(name.release.as_ptr()) }
        .to_bytes()
        .is_empty());
    assert!(!unsafe { CStr::from_ptr(name.machine.as_ptr()) }
        .to_bytes()
        .is_empty());
    assert_eq!(buffer.before, 0xA5A5_A5A5_A5A5_A5A5);
    assert_eq!(buffer.after, 0xA5A5_A5A5_A5A5_A5A5);
    assert_eq!(call(0), -(libc::EFAULT as i64));
    assert_eq!(call(1), -(libc::EFAULT as i64));
}
