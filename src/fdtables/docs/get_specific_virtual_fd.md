This is used to get a specific virtualfd mapping.

Useful for implementing something like dup2.  This closes the destination fd
if it exists, calling the relevant close handlers.   Use this only if you care 
which virtualfd you get.  Otherwise use [`get_unused_virtual_fd`].

Note, if you replace an entry which was the last reference to a realfd, with 
an entry with that same realfd, the intermediate close handler is called.

# Panics
  if the cageid does not exist

# Errors
  returns EBADF if it's not in the range of valid fds.

# Example
```
# use fdtables::*;
# let cage_id = threei::TESTING_CAGEID;
# init_empty_cage(cage_id);
# let underfd: u64 = 10;
# let fdkind: u32 = 0;
# let virtfd: u64 = 1000;
// Should not error...
assert!(get_specific_virtual_fd(cage_id, virtfd, fdkind, underfd, false, 0).is_ok());
// Check that you get the real fd back here...
assert_eq!(underfd,translate_virtual_fd(cage_id, virtfd).unwrap().underfd);
```
