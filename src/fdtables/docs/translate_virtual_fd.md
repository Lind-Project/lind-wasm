This is the main lookup function which takes a virtual fd and returns the entry

Converts a virtualfd, which is used in a cage, into the fdtableentry, which 
is used by whatever is below us.

# Panics
  if the cageid does not exist

# Errors
  if the virtualfd does not exist, the Result object has value EBADFD

# Returns 
  a `FDTableEntry` structure 

# Example
```
# use fdtables::*;
# let cage_id = threei::TESTING_CAGEID;
# let actualfd: u64 = 10;
# let fdkind: u32 = 0;
let my_virt_fd = get_unused_virtual_fd(cage_id, fdkind,actualfd, false, 0).unwrap();
// Check that you get the real fd back here...
assert_eq!(actualfd,translate_virtual_fd(cage_id, my_virt_fd).unwrap().underfd);
```
