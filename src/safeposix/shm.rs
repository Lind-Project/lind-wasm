// In map_shm function
interface::libc_mmap(
    shmaddr,
    self.size as usize,
    prot,
    ((MAP_SHARED as i32) | (MAP_FIXED as i32)),  // Cast each flag to i32 before combining
    fobjfdno,
    0,
)

// In unmap_shm function
interface::libc_mmap(
    shmaddr,
    self.size as usize,
    PROT_NONE,
    ((MAP_PRIVATE as i32) | (MAP_ANONYMOUS as i32) | (MAP_FIXED as i32)),  // Cast each flag to i32 before combining
    -1,
    0,
); 