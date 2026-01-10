Translate a bitmask returned by select into a virtual one for the caller

This is a helper function for select called after select is called.  After 
a select call returns, there are a series of bitmasks which need to be 
translated to virtualfd bitmasks (as this is what the caller expects).  
Also, a `HashSet`s of fds to add may be provided, which allows handling of 
fds you are virtually handling.  See also: [`prepare_bitmasks_for_select`] and
[`get_bitmask_for_select`].  (Note, you must use the same mapping table from 
your prior call when using this function.)

# Panics
  `mapping_table` is missing elements from the realfd's.
  nfds is larger than `FD_PER_PROCESS_MAX`

# Errors
  None

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

// select(....)  Suppose that fd 11 from the underlying select call was 
// readable...
# let nfds = 12;
# let mut selectfdbits = _init_fd_set();
# _fd_set(11,&mut selectfdbits);

// we would call:
let (amount, virtread) = get_one_virtual_bitmask_from_select_result(0,12,Some(selectfdbits), HashSet::new(), None, &mappingtable);
assert_eq!(amount,1);
```
