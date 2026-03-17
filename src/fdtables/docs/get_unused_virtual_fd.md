Get a virtualfd mapping to put an item into the fdtable.

This is the overwhelmingly common way to get a virtualfd and should be 
used essentially everywhere except in cases like `dup2()`, where you do 
actually care what fd you are assigned.

# Panics
  if the cageid does not exist

# Errors
  if the cage has used EMFILE virtual descriptors already, return EMFILE

# Example
```
# use fdtables::*;
# let cage_id = threei::TESTING_CAGEID;
# init_empty_cage(cage_id);
# let underfd: u64 = 10;
# let fdkind: u32 = 0;
// Should not error...
let my_virt_fd = get_unused_virtual_fd(cage_id, fdkind, underfd, false, 0).unwrap();
// Check that you get the real fd back here...
assert_eq!(underfd,translate_virtual_fd(cage_id, my_virt_fd).unwrap().underfd);
```
