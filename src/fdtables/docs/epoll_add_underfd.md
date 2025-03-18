Adds an underfd for a fdkind to a epollfd

This is a helper function for `epoll_create` and related functions.  It adds
an underfd for a fdkind into the epoll data structure.  You would use this
to handle situations where you want this fdkind to call the underlying
epoll call...

See also: [`epoll_create_empty`] and [`virtualize_epoll_ctl`].

# Panics
  cageid does not exist

  adding an underfd for an fdkind which already has one

# Errors
  EBADF if the epollfd is not a valid fd

  EINVAL if the epollfd is not actually an epollfd

# Example
```
# use fdtables::*;
# let cage_id = threei::TESTING_CAGEID4;
# init_empty_cage(cage_id);
let myepollfd = epoll_create_empty(cage_id,false).unwrap();

epoll_add_underfd(cage_id,myepollfd, 0, 10);

```
