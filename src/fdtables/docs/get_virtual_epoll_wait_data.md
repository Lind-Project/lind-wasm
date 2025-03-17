Returns virtual information from an epoll call so the caller can handle it

This call returns a hashmap indexed by fdkind.  Each of these values is itself
a hashmap which maps a virtualfd to an epoll event.   The caller must decide
how to handle this wait call.

See [`virtualize_epoll_ctl`] for more details.


# Panics
  cageid does not exist

# Errors
  EBADF  the epollfd doesn't exist.

  EINVAL the epollfd isn't an epoll file descriptor.


# Example
```
# use fdtables::*;
# let cage_id = threei::TESTING_CAGEID4;
# init_empty_cage(cage_id);
// make a fd we want to handle virtually...
let unrealfd = get_unused_virtual_fd(cage_id,1,10, false, 123).unwrap();

// let's create an epollfd which will watch it...
let myepollfd = epoll_create_empty(cage_id,false).unwrap();

let myevent = epoll_event {
    events: (EPOLLIN + EPOLLOUT) as u32,
    u64: 0,
};

// Add the unreal fd...
assert_eq!(virtualize_epoll_ctl(cage_id,myepollfd,EPOLL_CTL_ADD,unrealfd,myevent.clone()).unwrap(),());

// This should return the unrealfd's info!
assert!(get_virtual_epoll_wait_data(cage_id,myepollfd).unwrap().get(&1).unwrap().contains_key(&unrealfd));
```
