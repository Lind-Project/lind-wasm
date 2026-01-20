Translate select's bitmasks from virtual to the underlying fds.

This is a helper function for select, which is called before you make the
call to select.  It takes three virtual bitmasks Options and translates them
to hashmaps containing the underlying bitmask Options.  A None Option is just 
returned as None and is not processed.  The mapping table return value is 
needed to revert the `FDTableEntry`s back to virtualfds.


NOTE: If the same `FDTableEntry` is behind multiple virtualfds, only one of 
those virtualfds will be triggered.  I need to investigate how Linux behaves, 
but from what I can see from a quick search, the behavior here is undefined.

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
# init_empty_cage(cage_id);
// get_specific_virtual_fd(cage_id, VIRTFD, FDKIND, REALFD, CLOEXEC, OPTINFO)
get_specific_virtual_fd(cage_id, 3, 0, 11, false, 10).unwrap();
get_specific_virtual_fd(cage_id, 5, 0, 24, false, 123).unwrap();
get_specific_virtual_fd(cage_id, 7, 1, 15, false, 123).unwrap();
let mut fds_to_check= _init_fd_set();
_fd_set(3,&mut fds_to_check);
_fd_set(5,&mut fds_to_check);
_fd_set(7,&mut fds_to_check);
// map these into the right sets...
let (selectbittables, unparsedtables, mappingtable) = prepare_bitmasks_for_select(cage_id, 8, Some(fds_to_check), None, None,&HashSet::from([0])).unwrap();
// Should set the read to have bit 11 set...
assert!(_fd_isset(11,& selectbittables[0].get(&0).unwrap().1));
```
