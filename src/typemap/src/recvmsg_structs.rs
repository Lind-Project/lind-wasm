use std::mem;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct GuestMsghdr {
    pub msg_name: u32,
    pub msg_namelen: u32,
    pub msg_iov: u32,
    pub msg_iovlen: u32,
    pub msg_control: u32,
    pub msg_controllen: u32,
    pub msg_flags: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct GuestIovec {
    pub iov_base: u32,
    pub iov_len: u32,
}

const _: () = assert!(mem::size_of::<GuestMsghdr>() == 28 && mem::align_of::<GuestMsghdr>() == 4);
const _: () = assert!(mem::size_of::<GuestIovec>() == 8 && mem::align_of::<GuestIovec>() == 4);
