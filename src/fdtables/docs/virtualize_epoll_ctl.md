Modifies an epoll fd with a fd that is virtually handled.

This is a helper function for `epoll_ctl`.  It only operates on 
virtualfds which should be handled virtually (e.g., without calling epoll on
the grate or kernel layer below).  It returns () on success and an error,
if needed.

# Panics
  cageid does not exist

# Errors
  EBADF  epfd or fd is not a valid file descriptor.

  EEXIST op was `EPOLL_CTL_ADD`, and the supplied file descriptor fd
         is already registered with this epoll instance.

  EINVAL epfd is not an epoll file descriptor, or fd is the same as
         epfd, or the requested operation op is not supported by
         this interface.

  ELOOP  fd refers to an epoll instance and this `EPOLL_CTL_ADD`
         operation would result in a circular loop of epoll
         instances monitoring one another or a nesting depth of
         epoll instances greater than 5.

  ENOENT op was `EPOLL_CTL_MOD` or `EPOLL_CTL_DEL`, and fd is not
         registered with this epoll instance.

  Note, it is up to the caller to correctly understand when to call this 
function vs register an underfd and call below.

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

```
