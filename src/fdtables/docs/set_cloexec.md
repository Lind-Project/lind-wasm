Helper function for setting the close on exec (CLOEXEC) flag.

The reason this information is needed is because the [`empty_fds_for_exec`]
call needs to know which fds should be closed and which should be retained.

# Panics
  Unknown cageid

# Errors
  EBADFD if the virtual file descriptor is incorrect

# Example
```
# use fdtables::*;
# let cage_id = threei::TESTING_CAGEID;
# let fdkind: u32 = 0;
# let underfd: u64 = 10;
// Acquire a virtual fd...
let my_virt_fd = get_unused_virtual_fd(cage_id, fdkind, underfd, false, 100).unwrap();
// Swap this so it'll be closed when empty_fds_for_exec is called...
set_cloexec(cage_id, my_virt_fd, true).unwrap();
```
