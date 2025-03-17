/// THIS FILE NEEDS TO BE REFACTORED LATER!
///
use std::str::from_utf8;
use std::sync::atomic::{AtomicUsize, Ordering};

extern crate libc;

static mut UD_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum GenSockaddr {
    Unix(SockaddrUnix),
    V4(SockaddrV4),
    V6(SockaddrV6),
}
impl GenSockaddr {
    pub fn port(&self) -> u16 {
        match self {
            GenSockaddr::Unix(_) => panic!("Invalid function called for this type of Sockaddr."),
            GenSockaddr::V4(v4addr) => v4addr.sin_port,
            GenSockaddr::V6(v6addr) => v6addr.sin6_port,
        }
    }
    pub fn set_port(&mut self, port: u16) {
        match self {
            GenSockaddr::Unix(_) => panic!("Invalid function called for this type of Sockaddr."),
            GenSockaddr::V4(v4addr) => v4addr.sin_port = port,
            GenSockaddr::V6(v6addr) => v6addr.sin6_port = port,
        };
    }

    pub fn addr(&self) -> GenIpaddr {
        match self {
            GenSockaddr::Unix(_) => panic!("Invalid function called for this type of Sockaddr."),
            GenSockaddr::V4(v4addr) => GenIpaddr::V4(v4addr.sin_addr),
            GenSockaddr::V6(v6addr) => GenIpaddr::V6(v6addr.sin6_addr),
        }
    }

    pub fn set_addr(&mut self, ip: GenIpaddr) {
        match self {
            GenSockaddr::Unix(_unixaddr) => {
                panic!("Invalid function called for this type of Sockaddr.")
            }
            GenSockaddr::V4(v4addr) => {
                v4addr.sin_addr = if let GenIpaddr::V4(v4ip) = ip {
                    v4ip
                } else {
                    unreachable!()
                }
            }
            GenSockaddr::V6(v6addr) => {
                v6addr.sin6_addr = if let GenIpaddr::V6(v6ip) = ip {
                    v6ip
                } else {
                    unreachable!()
                }
            }
        };
    }

    pub fn set_family(&mut self, family: u16) {
        match self {
            GenSockaddr::Unix(unixaddr) => unixaddr.sun_family = family,
            GenSockaddr::V4(v4addr) => v4addr.sin_family = family,
            GenSockaddr::V6(v6addr) => v6addr.sin6_family = family,
        };
    }

    pub fn get_family(&self) -> u16 {
        match self {
            GenSockaddr::Unix(unixaddr) => unixaddr.sun_family,
            GenSockaddr::V4(v4addr) => v4addr.sin_family,
            GenSockaddr::V6(v6addr) => v6addr.sin6_family,
        }
    }

    pub fn path(&self) -> &str {
        match self {
            GenSockaddr::Unix(unixaddr) => {
                let pathiter = &mut unixaddr.sun_path.split(|idx| *idx == 0);
                let pathslice = pathiter.next().unwrap();
                let path = from_utf8(pathslice).unwrap();
                path
            }
            GenSockaddr::V4(_) => panic!("Invalid function called for this type of Sockaddr."),
            GenSockaddr::V6(_) => panic!("Invalid function called for this type of Sockaddr."),
        }
    }
}

#[derive(Debug, Hash, Eq, PartialEq, Clone, Copy)]
pub enum GenIpaddr {
    V4(V4Addr),
    V6(V6Addr),
}

impl GenIpaddr {
    pub fn is_unspecified(&self) -> bool {
        match self {
            GenIpaddr::V4(v4ip) => v4ip.s_addr == 0,
            GenIpaddr::V6(v6ip) => v6ip.s6_addr == [0; 16],
        }
    }
    pub fn from_string(string: &str) -> Option<Self> {
        let v4candidate: Vec<&str> = string.split('.').collect();
        let v6candidate: Vec<&str> = string.split(':').collect();
        let v4l = v4candidate.len();
        let v6l = v6candidate.len();
        if v4l == 1 && v6l > 1 {
            //then we should try parsing it as an ipv6 address
            let mut shortarr = [0u8; 16];
            let mut shortindex = 0;
            let mut encountered_doublecolon = false;
            for short in v6candidate {
                if short.is_empty() {
                    //you can only have a double colon once in an ipv6 address
                    if encountered_doublecolon {
                        return None;
                    }
                    encountered_doublecolon = true;

                    let numzeros = 8 - v6l + 1; //+1 to account for this empty string element
                    if numzeros == 0 {
                        return None;
                    }
                    shortindex += numzeros;
                } else {
                    //ok we can actually parse the element in this case
                    if let Ok(b) = short.parse::<u16>() {
                        //manually handle big endianness
                        shortarr[2 * shortindex] = (b >> 8) as u8;
                        shortarr[2 * shortindex + 1] = (b & 0xff) as u8;
                        shortindex += 1;
                    } else {
                        return None;
                    }
                }
            }
            return Some(Self::V6(V6Addr { s6_addr: shortarr }));
        } else if v4l == 4 && v6l == 1 {
            //then we should try parsing it as an ipv4 address
            let mut bytearr = [0u8; 4];
            let mut shortindex = 0;
            for byte in v4candidate {
                if let Ok(b) = byte.parse::<u8>() {
                    bytearr[shortindex] = b;
                    shortindex += 1;
                } else {
                    return None;
                }
            }
            return Some(Self::V4(V4Addr {
                s_addr: u32::from_ne_bytes(bytearr),
            }));
        } else {
            return None;
        }
    }
}

#[repr(C)]
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub struct SockaddrUnix {
    pub sun_family: u16,
    pub sun_path: [u8; 108],
}

impl Default for SockaddrUnix {
    fn default() -> Self {
        SockaddrUnix {
            sun_family: 0,
            sun_path: [0; 108],
        }
    }
}

pub fn new_sockaddr_unix(family: u16, path: &[u8]) -> SockaddrUnix {
    let pathlen = path.len();
    if pathlen > 108 {
        panic!("Unix domain paths cannot exceed 108 bytes.")
    }
    let mut array_path: [u8; 108] = [0; 108];
    array_path[0..pathlen].copy_from_slice(path);
    SockaddrUnix {
        sun_family: family,
        sun_path: array_path,
    }
}

pub fn gen_ud_path() -> String {
    let mut owned_path: String = "/sock".to_owned();
    unsafe {
        let id = UD_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        owned_path.push_str(&id.to_string());
    }
    owned_path.clone()
}

#[repr(C)]
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy, Default)]
pub struct V4Addr {
    pub s_addr: u32,
}
#[repr(C)]
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy, Default)]
pub struct SockaddrV4 {
    pub sin_family: u16,
    pub sin_port: u16,
    pub sin_addr: V4Addr,
    pub padding: u64,
}

#[repr(C)]
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy, Default)]
pub struct V6Addr {
    pub s6_addr: [u8; 16],
}
#[repr(C)]
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy, Default)]
pub struct SockaddrV6 {
    pub sin6_family: u16,
    pub sin6_port: u16,
    pub sin6_flowinfo: u32,
    pub sin6_addr: V6Addr,
    pub sin6_scope_id: u32,
}

#[derive(Debug, Default)]
#[repr(C)]
pub struct PollStruct {
    pub fd: i32,
    pub events: i16,
    pub revents: i16,
}

#[repr(C)]
pub struct SockaddrDummy {
    pub sa_family: u16,
    pub _sa_data: [u16; 14],
}
