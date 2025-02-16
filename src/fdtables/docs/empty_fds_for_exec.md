removes all entries with `should_cloexec` set with close handlers are called

This goes through every entry in a cage's fdtable and removes all entries
that have `should_cloexec` set to true.  These entries have the appropriate
close handlers called to handle this call.  See [`register_close_handlers`].

# Panics
  Invalid cageid

# Errors
  None

# Example
```
# use fdtables::*;
# let src_cage_id = threei::TESTING_CAGEID;
# let cage_id = threei::TESTING_CAGEID3;
# copy_fdtable_for_cage(src_cage_id,cage_id).unwrap();
let my_virt_fd = get_unused_virtual_fd(cage_id, 0, 20, true, 17).unwrap();
let my_virt_fd2 = get_unused_virtual_fd(cage_id, 0, 33, false, 16).unwrap();
empty_fds_for_exec(cage_id);
// The first fd should be closed, so isn't in the original table anymore...
assert!(translate_virtual_fd(cage_id, my_virt_fd).is_err());
// but the second one is!
assert!(translate_virtual_fd(cage_id, my_virt_fd2).is_ok());
```
