Get a virtualfd mapping to put an item into the fdtable.

This is used to request an unused fd from specific starting position. This is similar to 
`get_unused_virtual_fd_from_arg` except this function starts from a specific starting position 
mentioned by `arg` arguments. This will be used for `fcntl()`.

# Panics
  if the cageid does not exist

# Errors
  if the cage has used EMFILE virtual descriptors already, return EMFILE

# Example
```
# use fdtables::*;
# let cage_id = threei::TESTING_CAGEID;
# let underfd: u64 = 10;
# let fdkind: u32 = 0;
# let arg: u64 = 2; 
// Should not error... Ideally the `my_virt_fd` should be 3 in this case
let my_virt_fd = get_unused_virtual_fd_from_arg(cage_id, fdkind, underfd, false, 0, arg).unwrap();
// Check that you get the real fd back here...
assert_eq!(underfd,translate_virtual_fd(cage_id, my_virt_fd).unwrap().underfd);
```
