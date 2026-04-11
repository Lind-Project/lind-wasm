Check if a cageid has already been registered.

This is both an internal and exported helper function that checks whether a particular cageid has already been registered.

Internally, this is helpful for runtime assertions, externally it's helpful when modules have more complicated `fork/exec` setups, such as the IMFS interposing on these syscalls.

# Inputs
    `cageid: u64`   The cageid being checked.

# Returns
    `bool`: `true` if `cageid` is registered, `false` if not.


# Example
```
# use fdtables::*;
# let cage_id = threei::TESTING_CAGEID12;
init_empty_cage(cage_id);

assert!(check_cage_exists(cage_id));
```


