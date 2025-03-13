lets one get the underfds for an epollfd, so they can call epoll beneath

This is a helper function for epoll.  This lets a caller get a dict keyed
by fdkind, where the resulting value is the the corresponding underfd.
This is useful when you have an epoll call which will call down to a grate,
OS kernel, etc. beneath it.

# Panics
  cageid does not exist

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

// Ensure that fdkind 0, has the realfd 10...
assert_eq!(*epoll_get_underfd_hashmap(cage_id,myepollfd).unwrap().get(&0).unwrap(),10);

```
