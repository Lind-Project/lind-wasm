Get a virtualfd mapping to put an item into the fdtable.

This function requests an unused virtual FD starting from a specific position, as specified by the arg parameter. It behaves similarly to `get_unused_virtual_fd`, but starts the search at `arg` instead of `0`.

This is intended for use with `fcntl()` commands like `F_DUPFD` and `F_DUPFD_CLOEXEC`

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
# let arg: u64 = 2; 
// Should not error... Ideally the `my_virt_fd` should be 2 in this case
let my_virt_fd = get_unused_virtual_fd_from_startfd(cage_id, fdkind, underfd, false, 0, arg).unwrap();
// Check that you get the real fd back here...
assert_eq!(underfd,translate_virtual_fd(cage_id, my_virt_fd).unwrap().underfd);
```
