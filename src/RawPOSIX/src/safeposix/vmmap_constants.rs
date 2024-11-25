pub const PROT_READ: i32 = 0x1; /* Page can be read.  */
pub const PROT_WRITE: i32 = 0x2; /* Page can be written.  */
pub const PROT_EXEC: i32 = 0x4; /* Page can be executed.  */
pub const PROT_NONE: i32 = 0x0; /* Page can not be accessed.  */

pub const PROT_MASK: u32 = 0x7;

pub const MAP_SHARED: u32 = 0x01; /* Share changes.  */
pub const MAP_PRIVATE: u32 = 0x02; /* Changes are private.  */

/* this must be a multiple of the system page size */
pub const PAGESHIFT: u32 = 12;
pub const PAGESIZE: u32 = 1 << PAGESHIFT;

pub const MAP_PAGESHIFT: u32 = 16;
pub const MAP_PAGESIZE: u32 = 1 << MAP_PAGESHIFT;

pub const MAP_SHARING_MASK: u32 = 0x03;

pub const MAP_FIXED: u32 = 0x10; /* Interpret addr exactly.  */
pub const MAP_ANON: u32 = 0x20; /* Don't use a file.  */
pub const MAP_ANONYMOUS: u32 = MAP_ANON; /* Linux alias.  */

pub const MAP_FAILED: *mut std::ffi::c_void = (-1isize) as *mut std::ffi::c_void;

pub const MREMAP_MAYMOVE: u32 = 0x01;
pub const MREMAP_FIXED: u32 = 0x02;

pub const O_ACCMODE: i32 = 0003;
pub const O_RDONLY: i32 = 00;
pub const O_WRONLY: i32 = 01;
pub const O_RDWR: i32 = 02;
