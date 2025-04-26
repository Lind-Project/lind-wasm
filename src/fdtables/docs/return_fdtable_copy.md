gets a copy of a cage's fdtable hashmap

Utility function that some callers may want.  I'm not sure why this is 
needed exactly

# Panics
  Invalid cageid

# Errors
  None

# Example
```
# use fdtables::*;
# let cage_id = threei::TESTING_CAGEID;
let my_virt_fd = get_unused_virtual_fd(cage_id, 0, 10, false, 10).unwrap();
let my_cages_fdtable = return_fdtable_copy(cage_id);
assert!(my_cages_fdtable.get(&my_virt_fd).is_some());
// I can modify the cage table after this and the changes won't show up
// in my local HashMap since this is a copy...
```
