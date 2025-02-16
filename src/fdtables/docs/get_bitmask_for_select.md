Translate select's bitmasks for the different fdkinds.

This is a helper function for select, which prepares a single bitmask for use
with select.  Most likely, you want to call [`prepare_bitmasks_for_select`] 
instead.  A None Option is just returned as None and is not processed.  Also, 
only fdkind values which are listed in fdkinds have their bitmask created.  
Others are returned in the second item of the return tuple.  The mapping 
table return value is needed to revert the realfds back to virtualfds.


NOTE: If identical entries are behind multiple virtualfds, only one of those
virtualfds will be triggered.  I need to investigate how Linux behaves, but
from what I can see from a quick search, the behavior here is undefined.

# Panics
  Invalid cageid

# Errors
  This will return EBADF if any fd isn't valid
  This will return EINVAL if nfds is >= the maximum file descriptor limit

# Example
```
# use fdtables::*;
# use std::collections::HashSet;
# let cage_id = threei::TESTING_CAGEID;
// get_specific_virtual_fd(cage_id, VIRTFD, FDKIND, REALFD, CLOEXEC, OPTINFO)
get_specific_virtual_fd(cage_id, 3, 0, 11, false, 10).unwrap();
get_specific_virtual_fd(cage_id, 5, 0, 24, false, 123).unwrap();
get_specific_virtual_fd(cage_id, 7, 1, 15, false, 123).unwrap();
let mut fds_to_check= _init_fd_set();
_fd_set(3,&mut fds_to_check);
_fd_set(5,&mut fds_to_check);
_fd_set(7,&mut fds_to_check);
// map these into the right sets...
let (selectbithm, unparsedhm, mappingtable) = get_bitmask_for_select(cage_id, 8, Some(fds_to_check), &HashSet::from([0])).unwrap();
// Should set the read to have bit 11 set...
assert!(_fd_isset(11,& selectbithm.get(&0).unwrap().1));
// and the nfds should be 25 since 24 is the max and you need to add one
assert_eq!(selectbithm.get(&0).unwrap().0, 25);

assert_eq!(unparsedhm.len(), 1);
// One entry for each item
assert_eq!(mappingtable.len(), 3);
```
