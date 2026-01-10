Convert a poll or ppoll result's fdkind / underfd back to virtual

This is a helper function for poll / ppoll which is called after the actual
call is made.  It uses the mapping table from the previous 
[`convert_virtualfds_for_poll`] command to do this conversion.

# Panics
  None

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

assert_eq!(convert_poll_result_back_to_virtual(0,7,&mappingtable).unwrap(),3);


```
