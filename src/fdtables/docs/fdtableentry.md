The type of a stored table entry.  

This is the type of keys returned by [`/remove_cage_from_fdtable`], 
[`/empty_fds_for_exec`], and [`/return_fdtable_copy`].  It is likely the 
internal structure that is stored in each as well, but this is not required.

Note: One should check the realfds for `NO_REAL_FD` and EPOLLFD before assuming 
they are all valid.

