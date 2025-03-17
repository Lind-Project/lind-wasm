Set per file descriptor information needed by the library importer.  

This is useful to track things that are per-fd (like perhaps a non-block 
flag or table entry location).

# Panics
  Invalid cageid

# Errors
  BADFD if the virtualfd doesn't exist

# Example
```
# use fdtables::*;
# let cage_id = threei::TESTING_CAGEID;
# let actualfd: u64 = 10;
# let fdkind: u32 = 0;
let my_virt_fd = get_unused_virtual_fd(cage_id, fdkind,actualfd, false, 0).unwrap();
set_perfdinfo(cage_id, my_virt_fd,12345).unwrap();
assert_eq!(translate_virtual_fd(cage_id, my_virt_fd).unwrap().perfdinfo,12345);
```
