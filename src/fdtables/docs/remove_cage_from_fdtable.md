discards a cage (for `exit()`) and calls the appropriate close handlers

This is mostly used in handling exit, etc.  Calls all of the correct close
handlers.

# Panics
  Invalid cageid

# Errors
  None

# Example
```
# use fdtables::*;
# let src_cage_id = threei::TESTING_CAGEID;
# let cage_id = threei::TESTING_CAGEID2;
# copy_fdtable_for_cage(src_cage_id,cage_id).unwrap();
let my_virt_fd = get_unused_virtual_fd(cage_id, 0, 10, false, 10).unwrap();
remove_cage_from_fdtable(cage_id);
//   If we do the following line, it would panic, since the cage_id has 
//   been removed from the table...
// get_unused_virtual_fd(cage_id, 10, false, 10)
```
