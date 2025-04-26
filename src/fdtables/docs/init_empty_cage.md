Creates an empty cage's table in the fdtable.  No virtual fds are created.

This is useful for initialization and testing.  Creates an empty cage table.
See [`copy_fdtable_for_cage`] to handle `fork()`.

# Panics
  cageid is already used.

# Errors
  None

# Example
```
# use fdtables::*;
# let cage_id = threei::TESTING_CAGEID12;
init_empty_cage(cage_id);
// set up this cage's stdout for to go to the real stdout
get_specific_virtual_fd(cage_id, 0, 1, 1, false, 0).unwrap();
```
