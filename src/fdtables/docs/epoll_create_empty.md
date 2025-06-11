Creates a new fd for epoll and eppoll. 

This is a helper function for `epoll_create` and related functions.  It creates 
a new epoll fd which can be used by later calls, primarily 
[`epoll_add_underfd`] and [`virtualize_epoll_ctl`].

`epoll_add_underfd` is used to register an underfd to be called for future 
operations, which is useful when the grate / OS kernel should receive the 
epoll calls.

`virtualize_epoll_ctl` is used to add / modify / remove a fd from an epollfd
when the library wants to handle these fds internally / virtually.

# Panics
  cageid does not exist

# Errors
  EMFILE if there are no open file descriptors

# Example
```
# use fdtables::*;
# let cage_id = threei::TESTING_CAGEID4;
# init_empty_cage(cage_id);
let myepollfd = epoll_create_empty(cage_id,false).unwrap();

```
