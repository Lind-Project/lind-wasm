Convert virtual fds likely to handle the poll or ppoll command.

This is a helper function for poll / ppoll which is called before the actual
call is made.  It is given a cageid and a hashmap of virtualfds.  It returns
a fdkind indexed hashtable with a `HashSet` of (virtualfd, `FDTableEntry`) 
tuples.

There is also a mapping table (hashmap) returned, which is used to reverse 
this call.  For 
more info, see [`convert_poll_result_back_to_virtual`].  (Note, you must use 
the same mapping table from your prior call when using this function.)

# Panics
  unknown cageid

# Errors
  None

# Example
```
# use fdtables::*;
# use std::collections::HashSet;
# let cage_id = threei::TESTING_CAGEID;
# init_empty_cage(cage_id);
// get_specific_virtual_fd(cage_id, VIRTFD, FDKIND, UNDERFD, CLOEXEC, OPTINFO)
get_specific_virtual_fd(cage_id, 3, 0, 7, false, 10).unwrap();
get_specific_virtual_fd(cage_id, 5, 1, 10, false, 123).unwrap();

let (pollhashmap, mappingtable) = convert_virtualfds_for_poll(cage_id, HashSet::from([3,5]));

assert_eq!(pollhashmap.len(),2);

```
