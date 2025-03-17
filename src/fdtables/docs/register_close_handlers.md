Sets up user defined functions to be called when a `close()` happens.  

This lets a user function register itself to be called when a virtual fd is 
closed.  The arguments provided to the function are the entry and the count 
of remaining references.  

The first argument will be called when the count > 0, and the second argument
will be called on the last entry (count = 0).

# Panics
  Never

# Errors
  None

# Example
```should_panic
# use fdtables::*;
# let cage_id = threei::TESTING_CAGEID;
# let fdkind: u32 = 0;
# let underfd: u64 = 10;
# const MYVIRTFD:u64 = 123;
fn oh_no(_:FDTableEntry, _count:u64) {
    panic!("AAAARRRRGGGGHHHH!!!!");
}

// oh_no should be called when all references to the realfd are closed...
register_close_handlers(fdkind,NULL_FUNC,oh_no);

// Get a fd and dup it...
let my_virt_fd = get_unused_virtual_fd(cage_id, fdkind, underfd, false, 100).unwrap();
get_specific_virtual_fd(cage_id, MYVIRTFD, fdkind, underfd, false, 100).unwrap();

// Nothing should happen when I call this, since I'm closing only one reference
// and I registered the NULL_FUNC for this scenario...
close_virtualfd(cage_id,MYVIRTFD);
// However, after this, I will panic..
close_virtualfd(cage_id,my_virt_fd);
```
