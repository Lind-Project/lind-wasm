Duplicate a cage's fdtable -- useful for implementing `fork()`

This function is effectively just making a copy of a specific cage's
fdtable, for use in `fork()`.  Nothing complicated here.

# Panics
  Invalid cageid for srccageid
  Already used cageid for newcageid

# Errors
  This will return ENFILE if too many fds are used, if the implementation
  supports it...

# Example
```
# use fdtables::*;
# let src_cage_id = threei::TESTING_CAGEID;
# let new_cage_id = threei::TESTING_CAGEID1;
# init_empty_cage(src_cage_id);
let my_virt_fd = get_unused_virtual_fd(src_cage_id, 0, 10, false, 10).unwrap();
copy_fdtable_for_cage(src_cage_id,new_cage_id).unwrap();
// Check that this entry exists under the new_cage_id...
assert_eq!(translate_virtual_fd(new_cage_id, my_virt_fd).unwrap().underfd,10);
```
