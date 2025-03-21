Close a virtual file descriptor, calling the close handler, as appropriate

This is a helper function for close.  It calls the close handler which
is appropriate given the status of the underlying realfd.  See
[`register_close_handlers`] for more information.

# Panics
  Invalid cageid for srccageid

# Errors
  This will return EBADF if the fd isn't valid

# Example
```
# use fdtables::*;
# let cage_id = threei::TESTING_CAGEID;
# const underfd:u64 = 209;
# const fdkind:u32 = 0;
# const VIRTFD:u64= 345;
fn one(_:FDTableEntry,_:u64) { }
fn two(_:FDTableEntry,_:u64) { }
register_close_handlers(fdkind, one, two);
let my_virt_fd = get_unused_virtual_fd(cage_id, fdkind, underfd, false, 10).unwrap();
// dup2 call made for fd 15...
get_specific_virtual_fd(cage_id, VIRTFD, fdkind, underfd, false, 10).unwrap();
// Now they close the original fd...  This will call function "one"
close_virtualfd(cage_id,my_virt_fd).unwrap();
// both fds are closed.  This will call "two"
close_virtualfd(cage_id,VIRTFD).unwrap();
```
