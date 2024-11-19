#![allow(dead_code)]
// Network related system calls
// outlines and implements all of the networking system calls that are being emulated/faked in Lind

use super::fs_constants::*;
use super::net_constants::*;
use super::sys_constants::*;
use crate::interface;
use crate::interface::errnos::{syscall_error, Errno};
use crate::safeposix::cage::{FileDescriptor::*, *};
use crate::safeposix::filesystem::*;
use crate::safeposix::net::*;

impl Cage {
    //Initializes a socket file descriptor and sets the necessary flags
    fn _socket_initializer(
        &self,
        domain: i32,
        socktype: i32,
        protocol: i32,
        nonblocking: bool,
        cloexec: bool,
        conn: ConnState,
    ) -> SocketDesc {
        //For blocking sockets, operations wait until completed.
        //For non-blocking sockets, operations return immediately, even if the requested operation is not completed.
        //To further understand blocking, refer to https://www.scottklement.com/rpg/socktut/nonblocking.html
        //
        //O_CLOEXEC flag closes the fd pointing to the socket on the execution of a new program
        //This flag is neccessary upon setting the fd to avoid race condition described in
        //https://man7.org/linux/man-pages/man2/open.2.html under the O_CLOEXEC section
        let flags = if nonblocking { O_NONBLOCK } else { 0 } | if cloexec { O_CLOEXEC } else { 0 };

        let sockfd = SocketDesc {
            flags: flags,
            domain: domain,
            rawfd: -1, // RawFD set in bind for inet, or stays at -1 for others
            handle: interface::RustRfc::new(interface::RustLock::new(Self::mksockhandle(
                domain, socktype, protocol, conn, flags,
            ))),
            advlock: interface::RustRfc::new(interface::AdvisoryLock::new()),
        }; //currently on failure to create handle we create successfully but it's corrupted, change?

        return sockfd;
    }

    //Find an available file descriptor index in the File Descriptor Table
    //If available, insert the socket fd into the File Descriptor Table at the 
    //available index
    fn _socket_inserter(&self, sockfd: FileDescriptor) -> i32 {
        let (fd, guardopt) = self.get_next_fd(None);
        //In the case that no fd is available from the call to get_next_fd,
        //fd is set to -ENFILE = -23 and the error is propagated forward
        if fd < 0 {
            return fd;
        }
        let fdoption = &mut *guardopt.unwrap();
        //Insert the sockfd into the FileDescriptor option inside the fd table
        let _insertval = fdoption.insert(sockfd); 
        return fd;
    }

    //An implicit bind refers to the automatic binding of a socket to an address 
    //and port by the system, without an explicit call to the bind() function by the
    //programmer. This typically happens when the socket is used for client-side 
    //operations, such as when it initiates a connection to a server.
    fn _implicit_bind(&self, sockhandle: &mut SocketHandle, domain: i32) -> i32 {
        if sockhandle.localaddr.is_none() {
            //Assign a new local address to the socket handle
            let localaddr = match Self::assign_new_addr(
                sockhandle,
                domain,
                //The SO_RESUEPORT bit placement within the protocol int encodes rebind ability
                //The rebind ability of a socket refers to whether a socket can be 
                //re-bound to an address and port that it was previously bound to, 
                //especially after it has been closed or if the binding has been reset. 
                //To learn more about the importance of SO_REUSEPORT, check out https://lwn.net/Articles/542629/ 
                sockhandle.protocol & (1 << SO_REUSEPORT) != 0,
            ) {
                Ok(a) => a,
                Err(e) => return e,
            };

            //Bind the address to the socket handle
            let bindret = self.bind_inner_socket(sockhandle, &localaddr, true);

            //If an error occurs during binding,
            if bindret < 0 {
                match Errno::from_discriminant(interface::get_errno()) {
                    Ok(i) => {
                        return syscall_error(
                            i,
                            "recvfrom",
                            "syscall error from attempting to bind within recvfrom",
                        );
                    }
                    Err(()) => panic!("Unknown errno value from socket recvfrom returned!"),
                };
            }
        }
        0
    }

    pub fn socket_syscall(&self, domain: i32, socktype: i32, protocol: i32) -> i32 {
        let real_socktype = socktype & 0x7; //get the type without the extra flags, it's stored in the last 3 bits
        let nonblocking = (socktype & SOCK_NONBLOCK) != 0;
        let cloexec = (socktype & SOCK_CLOEXEC) != 0;

        match real_socktype {
            SOCK_STREAM => {
                //SOCK_STREAM defaults to TCP for protocol, otherwise protocol is unsupported
                let newprotocol = if protocol == 0 { IPPROTO_TCP } else { protocol };

                if newprotocol != IPPROTO_TCP {
                    return syscall_error(
                        Errno::EOPNOTSUPP,
                        "socket",
                        "The only SOCK_STREAM implemented is TCP. Unknown protocol input.",
                    );
                }
                match domain {
                    PF_INET | PF_UNIX => {
                        let sockfdobj = self._socket_initializer(
                            domain,
                            socktype,
                            newprotocol,
                            nonblocking,
                            cloexec,
                            ConnState::NOTCONNECTED,
                        );
                        return self._socket_inserter(Socket(sockfdobj));
                    }
                    _ => {
                        return syscall_error(
                            Errno::EOPNOTSUPP,
                            "socket",
                            "trying to use an unimplemented domain",
                        );
                    }
                }
            }

            SOCK_DGRAM => {
                //SOCK_DGRAM defaults to UDP for protocol, otherwise protocol is unsuported
                let newprotocol = if protocol == 0 { IPPROTO_UDP } else { protocol };

                if newprotocol != IPPROTO_UDP {
                    return syscall_error(
                        Errno::EOPNOTSUPP,
                        "socket",
                        "The only SOCK_DGRAM implemented is UDP. Unknown protocol input.",
                    );
                }
                match domain {
                    PF_INET | PF_UNIX => {
                        let sockfdobj = self._socket_initializer(
                            domain,
                            socktype,
                            newprotocol,
                            nonblocking,
                            cloexec,
                            ConnState::NOTCONNECTED,
                        );
                        return self._socket_inserter(Socket(sockfdobj));
                    }
                    _ => {
                        return syscall_error(
                            Errno::EOPNOTSUPP,
                            "socket",
                            "trying to use an unimplemented domain",
                        );
                    }
                }
            }

            _ => {
                return syscall_error(
                    Errno::EOPNOTSUPP,
                    "socket",
                    "trying to use an unimplemented socket type",
                );
            }
        }
    }

    //creates a sockhandle if none exists, otherwise this is a no-op
    pub fn force_innersocket(sockhandle: &mut SocketHandle) {
        //The innersocket is an Option wrapped around the raw sys fd
        //Depending on domain, innersocket may or may not be necessary
        //Ex: Unix sockets do not have a kernal fd
        //If innersocket is not available, then create a socket and insert its
        //fd into innersocket
        if let None = sockhandle.innersocket {
            //Create a new socket, as no sockhandle exists
            //Socket creation rarely fails except for invalid parameters or extremely low-resources conditions
            //Upon failure, process will panic
            //Lind handles IPv4, IPv6, and Unix for domains
            let thissock =
                interface::Socket::new(sockhandle.domain, sockhandle.socktype, sockhandle.protocol);

            //Loop through socket options and check which ones are set
            //This is necessary as we can only set one option at a time
            for reuse in [SO_REUSEPORT, SO_REUSEADDR] {
                //If socket option is not set, continue to next socket option
                if sockhandle.socket_options & (1 << reuse) == 0 {
                    continue;
                }

                //Otherwise, set the socket option in the new socket
                //The level argument specifies the protocol level at which the option resides
                //In our case, we are setting options at the socket level
                //To learn more about setsockopt https://man7.org/linux/man-pages/man3/setsockopt.3p.html
                let sockret = thissock.setsockopt(SOL_SOCKET, reuse, 1);
                //Failure occured upon setting a socket option
                //Possible failures can be read at the man page linked above
                //
                // TODO: Possibly add errors instead of having a single panic for all errors ??? ** //
                if sockret < 0 {
                    panic!("Cannot handle failure in setsockopt on socket creation");
                }
            }

            //Insert the socket file descriptor into the innersocket value
            sockhandle.innersocket = Some(thissock);
        };
    }

    //TODO: bind_syscall can be merged with bind_inner to be one function as this is a remnant 
    //from a previous refactor

    /// ### Description
    /// 
    /// `bind_syscall` - when a socket is created with socket_syscall, it exists in a name
    ///  space (address family) but has no address assigned to it. bind_syscall
    ///  assigns the address specified by localaddr to the socket referred to
    ///  by the file descriptor fd. 
    /// 
    /// ### Arguments
    /// 
    /// it accepts two parameters: 
    /// * `fd` - an open file descriptor
    /// * `localaddr` - the address to bind to the socket referred to by fd
    /// 
    /// ### Returns
    /// 
    /// On success, zero is returned.  On error, -errno is returned, and
    /// errno is set to indicate the error.
    /// 
    /// ### Errors
    /// 
    /// * EACCES - The address is protected, and the user is not the
    ///            superuser.
    /// * EADDRINUSE - The given address is already in use.
    /// * EADDRINUSE - (Internet domain sockets) The port number was specified as
    ///                zero in the socket address structure, but, upon attempting
    ///                to bind to an ephemeral port, it was determined that all
    ///                port numbers in the ephemeral port range are currently in
    ///                use.  See the discussion of
    ///                /proc/sys/net/ipv4/ip_local_port_range ip(7).
    /// * EBADF - sockfd is not a valid file descriptor.
    /// * EINVAL - The socket is already bound to an address.
    /// * EINVAL - addrlen is wrong, or addr is not a valid address for this
    ///            socket's domain.
    /// * ENOTSOCK - The file descriptor sockfd does not refer to a socket.
    ///
    /// The following errors are specific to UNIX domain (AF_UNIX)
    /// sockets:
    ///
    /// * EACCES - Search permission is denied on a component of the path
    ///            prefix.  (See also path_resolution(7).)
    /// * EADDRNOTAVAIL - A nonexistent interface was requested or the requested
    ///                   address was not local.
    /// * EFAULT - addr points outside the user's accessible address space.
    /// * ELOOP - Too many symbolic links were encountered in resolving
    ///           addr.
    /// * ENAMETOOLONG - addr is too long.
    /// * ENOENT - A component in the directory prefix of the socket pathname
    ///        does not exist.
    /// * ENOMEM - Insufficient kernel memory was available.
    /// * ENOTDIR - A component of the path prefix is not a directory.
    /// * EROFS - The socket inode would reside on a read-only filesystem.
    /// 
    // ** should this comment be left ?? : we assume we've converted into a RustSockAddr in the dispatcher
    pub fn bind_syscall(&self, fd: i32, localaddr: &interface::GenSockaddr) -> i32 {
        self.bind_inner(fd, localaddr, false)
    }

    //Direct to appropriate helper function based on the domain of the socket
    //to bind socket with local address
    //For INET sockets, create and bind the inner socket which is a kernel socket noted by raw_sys_fd
    //For Unix sockets, setup unix info field and bind to local address
    //On success, zero is returned.  On error, -errno is returned, and
    //errno is set to indicate the error.
    fn bind_inner_socket(
        &self,
        sockhandle: &mut SocketHandle,
        localaddr: &interface::GenSockaddr,
        prereserved: bool,
    ) -> i32 {
        //The family of the local address must match the domain of the socket handle
        if localaddr.get_family() != sockhandle.domain as u16 {
            return syscall_error(
                Errno::EINVAL,
                "bind",
                "An address with an invalid family for the given domain was specified",
            );
        }

        //If the socket is already bound to an address, exit with error
        if sockhandle.localaddr.is_some() {
            return syscall_error(
                Errno::EINVAL,
                "bind",
                "The socket is already bound to an address",
            );
        }

        let mut newsockaddr = localaddr.clone();

        //Bind socket based on domain type 
        //The rules used in name binding vary between address families.
        //To learn more, read under the description section at https://man7.org/linux/man-pages/man2/bind.2.html
        let res = match sockhandle.domain {
            AF_UNIX => self.bind_inner_socket_unix(sockhandle, &mut newsockaddr),
            AF_INET | AF_INET6 => {
                self.bind_inner_socket_inet(sockhandle, &mut newsockaddr, prereserved)
            }
            _ => {
                return syscall_error(Errno::EINVAL, "bind", "Unsupported domain provided");
            }
        };

        sockhandle.localaddr = Some(newsockaddr);

        res
    }

    //bind_syscall implementation in the case that the socket's domain is unix
    //More details at https://man7.org/linux/man-pages/man7/unix.7.html
    fn bind_inner_socket_unix(
        &self,
        sockhandle: &mut SocketHandle,
        newsockaddr: &mut interface::GenSockaddr,
    ) -> i32 {
        // Unix Sockets
        let path = newsockaddr.path();
        //Check that path is not empty
        if path.len() == 0 {
            return syscall_error(Errno::ENOENT, "bind", "given path was null");
        }
        //true path is normalized path of the path to a unix socket
        let truepath = normpath(convpath(path), self);

        //returns tuple consisting of inode number of file (if it exists), and 
        //inode number of parent (if it exists)
        match metawalkandparent(truepath.as_path()) {
            //If neither the file nor parent exists
            (None, None) => {
                return syscall_error(Errno::ENOENT, "bind", "a directory component in pathname does not exist or is a dangling symbolic link");
            }
            //If the file doesn't exist but the parent does
            (None, Some(pardirinode)) => {
                let filename = truepath.file_name().unwrap().to_str().unwrap().to_string(); //for now we assume this is sane, but maybe this should be checked later

                //this may end up skipping an inode number in the case of ENOTDIR, but that's not catastrophic
                //FS_METADATA contains information about the file system
                let newinodenum = FS_METADATA
                    .nextinode
                    .fetch_add(1, interface::RustAtomicOrdering::Relaxed); 
                    //fetch_add returns the previous value, which is the inode number we want, while incrementing the nextinode value as well
                    //The ordering argument Relaxed guarantees the memory location is atomic, which is all that is neccessary for a counter
                    //Read more at https://stackoverflow.com/questions/30407121/which-stdsyncatomicordering-to-use

                let newinode;

                //Pattern match the Directory Inode of the parent
                if let Inode::Dir(ref mut dir) =
                    *(FS_METADATA.inodetable.get_mut(&pardirinode).unwrap())
                {
                    //Add file type flags and user read,write,execute permission flags ***
                    let mode = (dir.mode | S_FILETYPEFLAGS as u32) & S_IRWXA;
                    //Add file type constant of a socket
                    let effective_mode = S_IFSOCK as u32 | mode;

                    let time = interface::timestamp(); //We do a real timestamp now
                    //Create a new inode for the file of the socket
                    newinode = Inode::Socket(SocketInode {
                        size: 0,
                        uid: DEFAULT_UID,
                        gid: DEFAULT_GID,
                        mode: effective_mode,
                        linkcount: 1,
                        refcount: 1,
                        atime: time,
                        ctime: time,
                        mtime: time,
                    });
                    //Find the DashMap that contains the parent directory
                    //Insert the file name and inode num as a key-value pair
                    //This will be used to find the inode num based on the file name
                    //in the file system
                    dir.filename_to_inode_dict
                        .insert(filename.clone(), newinodenum);
                    dir.linkcount += 1;
                } else {
                    //Parent dictory inode does not exist in inode table of file system
                    return syscall_error(
                        Errno::ENOTDIR,
                        "bind",
                        "unix domain socket path made socket address child of non-directory file",
                    );
                }
                //Insert unix info into socket handle
                //S_IFSOCK is the file type constant of a socket
                //0o666 allows read and write file operations within the directory
                //sendpipe and receivepipe are left as None because at the time of binding,
                //no data transfer occurs
                //inode is the newinodenum found above
                sockhandle.unix_info = Some(UnixSocketInfo {
                    mode: S_IFSOCK | 0o666,
                    sendpipe: None,
                    receivepipe: None,
                    inode: newinodenum,
                });

                //Insert path to socket file into a set
                NET_METADATA.domsock_paths.insert(truepath);
                //Insert the file inode num and inode as key-value pair into
                //file system inode table
                FS_METADATA.inodetable.insert(newinodenum, newinode);
            }
            //File already exists, meaning the given address argument to the bind_syscall 
            //is not available for the socket
            (Some(_inodenum), ..) => {
                return syscall_error(Errno::EADDRINUSE, "bind", "Address already in use");
            }
        }

        0
    }

    //bind_syscall implementation in the case that the socket's domain is INET
    //More details at https://man7.org/linux/man-pages/man7/ip.7.html
    fn bind_inner_socket_inet(
        &self,
        sockhandle: &mut SocketHandle,
        newsockaddr: &mut interface::GenSockaddr,
        prereserved: bool,
    ) -> i32 {
        //INET Sockets
        //rebind ability is set to true if the SO_REUSEPORT bit is set in 
        //the socket handle options
        let intent_to_rebind = sockhandle.socket_options & (1 << SO_REUSEPORT) != 0;
        //Create a socket and insert it into the innersocket in sockhandle
        Self::force_innersocket(sockhandle);

        //If socket address is preserved, set the local port to the reserved port
        //Otherwise, set the local port to a new value
        let newlocalport = if prereserved {
            newsockaddr.port()
        } else {
            //Reserve a new local port num
            let localout = NET_METADATA._reserve_localport(
                newsockaddr.addr(),
                newsockaddr.port(),
                sockhandle.protocol,
                sockhandle.domain,
                intent_to_rebind,
            );
            
            match localout {
                Err(errnum) => return errnum,
                Ok(local_port) => local_port,
            }
        };

        //Set the port of the socket address 
        newsockaddr.set_port(newlocalport);
        //Bind the address to the socket handle
        let bindret = sockhandle.innersocket.as_ref().unwrap().bind(&newsockaddr);

        //If an error occurs during binding, 
        if bindret < 0 {
            match Errno::from_discriminant(interface::get_errno()) {
                Ok(i) => {
                    return syscall_error(i, "bind", "The libc call to bind failed!");
                }
                Err(()) => panic!("Unknown errno value from socket bind returned!"),
            };
        }

        0
    }

    //Helper function of bind_syscall
    //Checks if fd refers to a valid socket file descriptor
    //fd: the file descriptor associated with the socket
    //localaddr: reference to the GenSockaddr enum that hold the address
    //prereserved: bool that describes whether the address and port have 
    //             been set aside or designated for specific purposes
    pub fn bind_inner(
        &self,
        fd: i32,
        localaddr: &interface::GenSockaddr,
        prereserved: bool,
    ) -> i32 {
        //checkedfd is an atomic reference count of the number of locks on the fd
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        //returns a write lock once no other writers or readers have access to the lock
        let mut unlocked_fd = checkedfd.write();
        if let Some(filedesc_enum) = &mut *unlocked_fd {
            match filedesc_enum {
                //*unlocked_fd is in the format of Option<T>, where T is of type Socket(&mut SocketDesc)
                Socket(ref mut sockfdobj) => {
                    //Clone the socket handle
                    let sock_tmp = sockfdobj.handle.clone();
                    //Obtain write gaurd for socket handle
                    let mut sockhandle = sock_tmp.write();
                    //We would like to pass the socket handle data to the function
                    //without giving it ownership sockfdobj, which may be in use
                    //by other threads accessing other fields
                    self.bind_inner_socket(&mut *sockhandle, localaddr, prereserved)
                }
                //error if file descriptor doesn't refer to a socket
                _ => syscall_error(
                    Errno::ENOTSOCK,
                    "bind",
                    "file descriptor refers to something other than a socket",
                ),
            }
        //error if fd is invalid
        } else {
            syscall_error(Errno::EBADF, "bind", "invalid file descriptor")
        }
    }

    fn assign_new_addr_unix(sockhandle: &SocketHandle) -> interface::GenSockaddr {
        if let Some(addr) = sockhandle.localaddr.clone() {
            addr
        } else {
            let path = interface::gen_ud_path();
            let newremote = interface::GenSockaddr::Unix(interface::new_sockaddr_unix(
                AF_UNIX as u16,
                path.as_bytes(),
            ));
            newremote
        }
    }

    fn assign_new_addr(
        sockhandle: &SocketHandle,
        domain: i32,
        rebindability: bool,
    ) -> Result<interface::GenSockaddr, i32> {
        if let Some(addr) = &sockhandle.localaddr {
            Ok(addr.clone())
        } else {
            let mut newremote: interface::GenSockaddr;
            //This is the specified behavior for the berkeley sockets API
            match domain {
                AF_UNIX => {
                    let path = interface::gen_ud_path();
                    newremote = interface::GenSockaddr::Unix(interface::new_sockaddr_unix(
                        AF_UNIX as u16,
                        path.as_bytes(),
                    ));
                }
                AF_INET => {
                    newremote = interface::GenSockaddr::V4(interface::SockaddrV4::default());
                    let addr = interface::GenIpaddr::V4(interface::V4Addr::default());
                    newremote.set_addr(addr);
                    newremote.set_family(AF_INET as u16);
                    newremote.set_port(
                        match NET_METADATA._reserve_localport(
                            addr.clone(),
                            0,
                            sockhandle.protocol,
                            sockhandle.domain,
                            rebindability,
                        ) {
                            Ok(portnum) => portnum,
                            Err(errnum) => return Err(errnum),
                        },
                    );
                }
                AF_INET6 => {
                    newremote = interface::GenSockaddr::V6(interface::SockaddrV6::default());
                    let addr = interface::GenIpaddr::V6(interface::V6Addr::default());
                    newremote.set_addr(addr);
                    newremote.set_family(AF_INET6 as u16);
                    newremote.set_port(
                        match NET_METADATA._reserve_localport(
                            addr.clone(),
                            0,
                            sockhandle.protocol,
                            sockhandle.domain,
                            rebindability,
                        ) {
                            Ok(portnum) => portnum,
                            Err(errnum) => return Err(errnum),
                        },
                    );
                }
                _ => {
                    return Err(syscall_error(
                        Errno::EOPNOTSUPP,
                        "assign",
                        "Unkown protocol when assigning",
                    ));
                }
            };
            Ok(newremote)
        }
    }

    //The connect() system call connects the socket referred to by the
    //file descriptor fd to the address specified by remoteaddr.
    pub fn connect_syscall(&self, fd: i32, remoteaddr: &interface::GenSockaddr) -> i32 {
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let mut unlocked_fd = checkedfd.write();
        if let Some(filedesc_enum) = &mut *unlocked_fd {
            match filedesc_enum {
                Socket(ref mut sockfdobj) => {
                    let sock_tmp = sockfdobj.handle.clone();
                    let mut sockhandle = sock_tmp.write();
                    if remoteaddr.get_family() != sockhandle.domain as u16 {
                        return syscall_error(
                            Errno::EINVAL,
                            "connect",
                            "An address with an invalid family for the given domain was specified",
                        );
                    }

                    match sockhandle.protocol {
                        IPPROTO_UDP => {
                            return self.connect_udp(&mut *sockhandle, sockfdobj, remoteaddr)
                        }
                        IPPROTO_TCP => {
                            return self.connect_tcp(&mut *sockhandle, sockfdobj, remoteaddr)
                        }
                        _ => {
                            return syscall_error(
                                Errno::EOPNOTSUPP,
                                "connect",
                                "Unknown protocol in connect",
                            )
                        }
                    };
                }
                _ => {
                    return syscall_error(
                        Errno::ENOTSOCK,
                        "connect",
                        "file descriptor refers to something other than a socket",
                    );
                }
            }
        } else {
            return syscall_error(Errno::EBADF, "connect", "invalid file descriptor");
        }
    }

    fn connect_udp(
        &self,
        sockhandle: &mut SocketHandle,
        sockfdobj: &mut SocketDesc,
        remoteaddr: &interface::GenSockaddr,
    ) -> i32 {
        //for UDP, just set the addresses and return
        //we don't need to check connection state for UDP, it's connectionless!
        sockhandle.remoteaddr = Some(remoteaddr.clone());
        match sockhandle.localaddr {
            Some(_) => return 0,
            None => {
                let localaddr = match Self::assign_new_addr(
                    &*sockhandle,
                    sockhandle.domain,
                    sockhandle.protocol & (1 << SO_REUSEPORT) != 0,
                ) {
                    Ok(a) => a,
                    Err(e) => return e,
                };

                let bindret = self.bind_inner_socket(&mut *sockhandle, &localaddr, true);
                // udp now connected so lets set rawfd for select
                sockfdobj.rawfd = sockhandle.innersocket.as_ref().unwrap().raw_sys_fd;
                return bindret;
            }
        };
    }

    fn connect_tcp(
        &self,
        sockhandle: &mut SocketHandle,
        sockfdobj: &mut SocketDesc,
        remoteaddr: &interface::GenSockaddr,
    ) -> i32 {
        // TCP connection logic
        if sockhandle.state != ConnState::NOTCONNECTED {
            return syscall_error(
                Errno::EISCONN,
                "connect",
                "The descriptor is already connected",
            );
        }

        match sockhandle.domain {
            AF_UNIX => self.connect_tcp_unix(&mut *sockhandle, sockfdobj, remoteaddr),
            AF_INET | AF_INET6 => self.connect_tcp_inet(&mut *sockhandle, sockfdobj, remoteaddr),
            _ => return syscall_error(Errno::EINVAL, "connect", "Unsupported domain provided"),
        }
    }

    fn connect_tcp_unix(
        &self,
        sockhandle: &mut SocketHandle,
        sockfdobj: &mut SocketDesc,
        remoteaddr: &interface::GenSockaddr,
    ) -> i32 {
        // TCP domain socket logic
        if let None = sockhandle.localaddr {
            let localaddr = Self::assign_new_addr_unix(&sockhandle);
            self.bind_inner_socket(&mut *sockhandle, &localaddr, false);
        }
        let remotepathbuf = normpath(convpath(remoteaddr.path()), self);

        // try to get and hold reference to the key-value pair, so other process can't alter it
        let path_ref = NET_METADATA.domsock_paths.get(&remotepathbuf);
        // if the entry doesn't exist, return an error.
        if path_ref.is_none() {
            return syscall_error(Errno::ENOENT, "connect", "not valid unix domain path");
        }

        let (pipe1, pipe2) = create_unix_sockpipes();

        sockhandle.remoteaddr = Some(remoteaddr.clone());
        sockhandle.unix_info.as_mut().unwrap().sendpipe = Some(pipe1.clone());
        sockhandle.unix_info.as_mut().unwrap().receivepipe = Some(pipe2.clone());

        let connvar = if sockfdobj.flags & O_NONBLOCK == 0 {
            Some(interface::RustRfc::new(ConnCondVar::new()))
        } else {
            None
        };

        // receive_pipe and send_pipe need to be swapped here
        // because the receive_pipe and send_pipe are opposites between the
        // sender and receiver. Swapping here also means we do not need to swap in
        // accept.
        let entry = DomsockTableEntry {
            sockaddr: sockhandle.localaddr.unwrap().clone(),
            receive_pipe: Some(pipe1.clone()).unwrap(),
            send_pipe: Some(pipe2.clone()).unwrap(),
            cond_var: connvar.clone(),
        };
        NET_METADATA
            .domsock_accept_table
            .insert(remotepathbuf, entry);
        sockhandle.state = ConnState::CONNECTED;
        if sockfdobj.flags & O_NONBLOCK == 0 {
            connvar.unwrap().wait();
        }
        return 0;
    }

    fn connect_tcp_inet(
        &self,
        sockhandle: &mut SocketHandle,
        sockfdobj: &mut SocketDesc,
        remoteaddr: &interface::GenSockaddr,
    ) -> i32 {
        // TCP inet domain logic
        //for TCP, actually create the internal socket object and connect it
        let remoteclone = remoteaddr.clone();

        if sockhandle.state != ConnState::NOTCONNECTED {
            return syscall_error(
                Errno::EISCONN,
                "connect",
                "The descriptor is already connected",
            );
        }

        if let None = sockhandle.localaddr {
            Self::force_innersocket(sockhandle);

            let localaddr = match Self::assign_new_addr(
                &*sockhandle,
                sockhandle.domain,
                sockhandle.protocol & (1 << SO_REUSEPORT) != 0,
            ) {
                Ok(a) => a,
                Err(e) => return e,
            };
            let bindret = sockhandle.innersocket.as_ref().unwrap().bind(&localaddr);
            if bindret < 0 {
                sockhandle.localaddr = Some(localaddr);
                match Errno::from_discriminant(interface::get_errno()) {
                    Ok(i) => {
                        return syscall_error(
                            i,
                            "connect",
                            "The libc call to bind within connect failed",
                        );
                    }
                    Err(()) => {
                        panic!("Unknown errno value from socket bind within connect returned!")
                    }
                };
            }
        }

        let mut inprogress = false;
        let connectret = sockhandle
            .innersocket
            .as_ref()
            .unwrap()
            .connect(&remoteclone);
        if connectret < 0 {
            match Errno::from_discriminant(interface::get_errno()) {
                Ok(i) => {
                    if i == Errno::EINPROGRESS {
                        inprogress = true;
                    } else {
                        return syscall_error(i, "connect", "The libc call to connect failed!");
                    };
                }
                Err(()) => panic!("Unknown errno value from socket connect returned!"),
            };
        }

        sockhandle.state = ConnState::CONNECTED;
        sockhandle.remoteaddr = Some(remoteaddr.clone());
        sockhandle.errno = 0;
        // set the rawfd for select
        sockfdobj.rawfd = sockhandle.innersocket.as_ref().unwrap().raw_sys_fd;
        if inprogress {
            sockhandle.state = ConnState::INPROGRESS;
            return syscall_error(
                Errno::EINPROGRESS,
                "connect",
                "The libc call to connect is in progress.",
            );
        } else {
            return 0;
        }
    }

    fn mksockhandle(
        domain: i32,
        socktype: i32,
        protocol: i32,
        conn: ConnState,
        socket_options: i32,
    ) -> SocketHandle {
        SocketHandle {
            innersocket: None,
            socket_options: socket_options,
            tcp_options: 0,
            state: conn,
            protocol: protocol,
            domain: domain,
            last_peek: interface::RustDeque::new(),
            localaddr: None,
            remoteaddr: None,
            unix_info: None,
            socktype: socktype,
            sndbuf: 131070, //buffersize, which is only used by getsockopt
            rcvbuf: 262140, //buffersize, which is only used by getsockopt
            errno: 0,
        }
    }

    pub fn sendto_syscall(
        &self,
        fd: i32,
        buf: *const u8,
        buflen: usize,
        flags: i32,
        dest_addr: &interface::GenSockaddr,
    ) -> i32 {
        //if ip and port are not specified, shunt off to send
        if dest_addr.port() == 0 && dest_addr.addr().is_unspecified() {
            return self.send_syscall(fd, buf, buflen, flags);
        }

        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let mut unlocked_fd = checkedfd.write();
        if let Some(filedesc_enum) = &mut *unlocked_fd {
            match filedesc_enum {
                Socket(ref mut sockfdobj) => {
                    let sock_tmp = sockfdobj.handle.clone();
                    let mut sockhandle = sock_tmp.write();

                    // check if this is a domain socket
                    if sockhandle.domain == AF_UNIX {
                        return syscall_error(
                            Errno::EISCONN,
                            "sendto",
                            "The descriptor is connection-oriented",
                        );
                    }

                    if dest_addr.get_family() != sockhandle.domain as u16 {
                        return syscall_error(
                            Errno::EINVAL,
                            "sendto",
                            "An address with an invalid family for the given domain was specified",
                        );
                    }
                    if (flags & !MSG_NOSIGNAL) != 0 {
                        return syscall_error(
                            Errno::EOPNOTSUPP,
                            "sendto",
                            "The flags are not understood!",
                        );
                    }

                    if sockhandle.state != ConnState::NOTCONNECTED {
                        return syscall_error(
                            Errno::EISCONN,
                            "sendto",
                            "The descriptor is connected",
                        );
                    }

                    match sockhandle.protocol {
                        //Sendto doesn't make sense for the TCP protocol, it's connection oriented
                        IPPROTO_TCP => {
                            return syscall_error(
                                Errno::EISCONN,
                                "sendto",
                                "The descriptor is connection-oriented",
                            );
                        }

                        IPPROTO_UDP => {
                            let tmpdest = *dest_addr;
                            let ibindret =
                                self._implicit_bind(&mut *sockhandle, tmpdest.get_family() as i32);
                            if ibindret < 0 {
                                return ibindret;
                            }

                            //unwrap ok because we implicit_bind_right before
                            let sockret = sockhandle.innersocket.as_ref().unwrap().sendto(
                                buf,
                                buflen,
                                Some(dest_addr),
                            );

                            //we don't mind if this fails for now and we will just get the error
                            //from calling sendto
                            if sockret < 0 {
                                match Errno::from_discriminant(interface::get_errno()) {
                                    Ok(i) => {
                                        return syscall_error(
                                            i,
                                            "sendto",
                                            "The libc call to sendto failed!",
                                        );
                                    }
                                    Err(()) => {
                                        panic!("Unknown errno value from socket sendto returned!")
                                    }
                                };
                            } else {
                                return sockret;
                            }
                        }

                        _ => {
                            return syscall_error(
                                Errno::EOPNOTSUPP,
                                "sendto",
                                "Unkown protocol in sendto",
                            );
                        }
                    }
                }

                _ => {
                    return syscall_error(
                        Errno::ENOTSOCK,
                        "sendto",
                        "file descriptor refers to something other than a socket",
                    );
                }
            }
        } else {
            return syscall_error(Errno::EBADF, "sendto", "invalid file descriptor");
        }
    }

    pub fn send_syscall(&self, fd: i32, buf: *const u8, buflen: usize, flags: i32) -> i32 {
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let mut unlocked_fd = checkedfd.write();
        if let Some(filedesc_enum) = &mut *unlocked_fd {
            match filedesc_enum {
                Socket(ref mut sockfdobj) => {
                    let sock_tmp = sockfdobj.handle.clone();
                    let sockhandle = sock_tmp.write();

                    if (flags & !MSG_NOSIGNAL) != 0 {
                        return syscall_error(
                            Errno::EOPNOTSUPP,
                            "send",
                            "The flags are not understood!",
                        );
                    }

                    // check if this is a domain socket
                    let socket_type = sockhandle.domain;
                    match socket_type {
                        AF_UNIX => {
                            match sockhandle.protocol {
                                IPPROTO_TCP => {
                                    // to be able to send here we either need to be fully connected, or connected for write only
                                    if (sockhandle.state != ConnState::CONNECTED)
                                        && (sockhandle.state != ConnState::CONNWRONLY)
                                    {
                                        return syscall_error(
                                            Errno::ENOTCONN,
                                            "writev",
                                            "The descriptor is not connected",
                                        );
                                    }
                                    // get the socket pipe, write to it, and return bytes written
                                    let sockinfo = &sockhandle.unix_info.as_ref().unwrap();
                                    let mut nonblocking = false;
                                    if sockfdobj.flags & O_NONBLOCK != 0 {
                                        nonblocking = true;
                                    }
                                    let retval = match sockinfo.sendpipe.as_ref() {
                                        Some(sendpipe) => {
                                            sendpipe.write_to_pipe(buf, buflen, nonblocking) as i32
                                        }
                                        None => {
                                            return syscall_error(Errno::EAGAIN, "writev", "there is no data available right now, try again later");
                                        }
                                    };
                                    if retval == -(Errno::EPIPE as i32) {
                                        interface::lind_kill_from_id(self.cageid, SIGPIPE);
                                    } // Trigger SIGPIPE
                                    retval
                                }
                                _ => {
                                    return syscall_error(
                                        Errno::EOPNOTSUPP,
                                        "send",
                                        "Unkown protocol in send",
                                    );
                                }
                            }
                        }
                        // for inet
                        AF_INET | AF_INET6 => match sockhandle.protocol {
                            IPPROTO_TCP => {
                                if (sockhandle.state != ConnState::CONNECTED)
                                    && (sockhandle.state != ConnState::CONNWRONLY)
                                {
                                    return syscall_error(
                                        Errno::ENOTCONN,
                                        "send",
                                        "The descriptor is not connected",
                                    );
                                }

                                //because socket must be connected it must have an inner socket
                                let retval = sockhandle
                                    .innersocket
                                    .as_ref()
                                    .unwrap()
                                    .sendto(buf, buflen, None);
                                if retval < 0 {
                                    match Errno::from_discriminant(interface::get_errno()) {
                                        Ok(i) => {
                                            return syscall_error(
                                                i,
                                                "send",
                                                "The libc call to sendto failed!",
                                            );
                                        }
                                        Err(()) => panic!(
                                            "Unknown errno value from socket sendto returned!"
                                        ),
                                    };
                                } else {
                                    return retval;
                                }
                            }

                            IPPROTO_UDP => {
                                let remoteaddr = match &sockhandle.remoteaddr {
                                    Some(x) => x.clone(),
                                    None => {
                                        return syscall_error(
                                            Errno::ENOTCONN,
                                            "send",
                                            "The descriptor is not connected",
                                        );
                                    }
                                };
                                drop(unlocked_fd);
                                drop(sockhandle);
                                //send from a udp socket is just shunted off to sendto with the remote address set
                                return self.sendto_syscall(fd, buf, buflen, flags, &remoteaddr);
                            }

                            _ => {
                                return syscall_error(
                                    Errno::EOPNOTSUPP,
                                    "send",
                                    "Unkown protocol in send",
                                );
                            }
                        },
                        _ => {
                            return syscall_error(
                                Errno::EINVAL,
                                "connect",
                                "Unsupported domain provided",
                            )
                        }
                    }
                }
                _ => {
                    return syscall_error(
                        Errno::ENOTSOCK,
                        "send",
                        "file descriptor refers to something other than a socket",
                    );
                }
            }
        } else {
            return syscall_error(Errno::EBADF, "send", "invalid file descriptor");
        }
    }

    fn recv_common_inner(
        &self,
        filedesc_enum: &mut FileDescriptor,
        buf: *mut u8,
        buflen: usize,
        flags: i32,
        addr: &mut Option<&mut interface::GenSockaddr>,
    ) -> i32 {
        match &mut *filedesc_enum {
            Socket(ref mut sockfdobj) => {
                let sock_tmp = sockfdobj.handle.clone();
                let mut sockhandle = sock_tmp.write();
                match sockhandle.protocol {
                    IPPROTO_TCP => {
                        return self.recv_common_inner_tcp(
                            &mut sockhandle,
                            sockfdobj,
                            buf,
                            buflen,
                            flags,
                            addr,
                        )
                    }
                    IPPROTO_UDP => {
                        return self.recv_common_inner_udp(
                            &mut sockhandle,
                            sockfdobj,
                            buf,
                            buflen,
                            addr,
                        )
                    }

                    _ => {
                        return syscall_error(
                            Errno::EOPNOTSUPP,
                            "recvfrom",
                            "Unkown protocol in recvfrom",
                        );
                    }
                }
            }
            _ => {
                return syscall_error(
                    Errno::ENOTSOCK,
                    "recvfrom",
                    "file descriptor refers to something other than a socket",
                );
            }
        }
    }

    fn recv_common_inner_tcp(
        &self,
        sockhandle: &mut interface::RustLockWriteGuard<SocketHandle>,
        sockfdobj: &mut SocketDesc,
        buf: *mut u8,
        buflen: usize,
        flags: i32,
        addr: &mut Option<&mut interface::GenSockaddr>,
    ) -> i32 {
        // maybe select reported a INPROGRESS tcp socket as readable, so re-check the state here
        if sockhandle.state == ConnState::INPROGRESS
            && sockhandle
                .innersocket
                .as_ref()
                .unwrap()
                .check_rawconnection()
        {
            sockhandle.state = ConnState::CONNECTED;
        }

        if (sockhandle.state != ConnState::CONNECTED) && (sockhandle.state != ConnState::CONNRDONLY)
        {
            return syscall_error(
                Errno::ENOTCONN,
                "recvfrom",
                "The descriptor is not connected",
            );
        }

        let mut newbuflen = buflen;
        let mut newbufptr = buf;

        //if we have peeked some data before, fill our buffer with that data before moving on
        if !sockhandle.last_peek.is_empty() {
            let bytecount = interface::rust_min(sockhandle.last_peek.len(), newbuflen);
            interface::copy_fromrustdeque_sized(buf, bytecount, &sockhandle.last_peek);
            newbuflen -= bytecount;
            newbufptr = newbufptr.wrapping_add(bytecount);

            //if we're not still peeking data, consume the data we peeked from our peek buffer
            //and if the bytecount is more than the length of the peeked data, then we remove the entire
            //buffer
            if flags & MSG_PEEK == 0 {
                let len = sockhandle.last_peek.len();
                sockhandle
                    .last_peek
                    .drain(..(if bytecount > len { len } else { bytecount }));
            }

            if newbuflen == 0 {
                //if we've filled all of the buffer with peeked data, return
                return bytecount as i32;
            }
        }

        let bufleft = newbufptr;
        let buflenleft = newbuflen;
        let mut retval;

        if sockhandle.domain == AF_UNIX {
            // get the remote socket pipe, read from it, and return bytes read
            let mut nonblocking = false;
            if sockfdobj.flags & O_NONBLOCK != 0 {
                nonblocking = true;
            }
            loop {
                let sockinfo = &sockhandle.unix_info.as_ref().unwrap();
                let receivepipe = sockinfo.receivepipe.as_ref().unwrap();
                retval = receivepipe.read_from_pipe(bufleft, buflenleft, nonblocking) as i32;
                if retval < 0 {
                    //If we have already read from a peek but have failed to read more, exit!
                    if buflen != buflenleft {
                        return (buflen - buflenleft) as i32;
                    }
                    if sockfdobj.flags & O_NONBLOCK == 0 && retval == -(Errno::EAGAIN as i32) {
                        // with blocking sockets, we return EAGAIN here to check for cancellation, then return to reading
                        if self
                            .cancelstatus
                            .load(interface::RustAtomicOrdering::Relaxed)
                        {
                            // if the cancel status is set in the cage, we trap around a cancel point
                            // until the individual thread is signaled to cancel itself
                            loop {
                                interface::cancelpoint(self.cageid)
                            }
                        }
                        // in order to prevent deadlock
                        interface::RustLockWriteGuard::<SocketHandle>::bump(sockhandle);
                        continue;
                    } else {
                        //if not EAGAIN, return the error
                        return retval;
                    }
                }
                break;
            }
        } else {
            loop {
                // we loop here so we can cancel blocking recvs
                //socket must be connected so unwrap ok
                if sockfdobj.flags & O_NONBLOCK != 0 {
                    retval = sockhandle
                        .innersocket
                        .as_ref()
                        .unwrap()
                        .recvfrom_nonblocking(bufleft, buflenleft, addr);
                } else {
                    retval = sockhandle
                        .innersocket
                        .as_ref()
                        .unwrap()
                        .recvfrom(bufleft, buflenleft, addr);
                }

                if retval < 0 {
                    //If we have already read from a peek but have failed to read more, exit!
                    if buflen != buflenleft {
                        return (buflen - buflenleft) as i32;
                    }

                    match Errno::from_discriminant(interface::get_errno()) {
                        Ok(i) => {
                            //We have the recieve timeout set to every one second, so
                            //if our blocking socket ever returns EAGAIN, it must be
                            //the case that this recv timeout was exceeded, and we
                            //should thus not treat this as a failure in our emulated
                            //socket; see comment in Socket::new in interface/comm.rs
                            if sockfdobj.flags & O_NONBLOCK == 0 && i == Errno::EAGAIN {
                                if self
                                    .cancelstatus
                                    .load(interface::RustAtomicOrdering::Relaxed)
                                {
                                    // if the cancel status is set in the cage, we trap around a cancel point
                                    // until the individual thread is signaled to cancel itself
                                    loop {
                                        interface::cancelpoint(self.cageid);
                                    }
                                }
                                interface::RustLockWriteGuard::<SocketHandle>::bump(sockhandle);
                                continue; // EAGAIN, try again
                            }

                            return syscall_error(
                                i,
                                "recvfrom",
                                "Internal call to recvfrom failed",
                            );
                        }
                        Err(()) => panic!("Unknown errno value from socket recvfrom returned!"),
                    };
                }
                break; // we're okay to move on
            }
        }
        let totalbyteswritten = (buflen - buflenleft) as i32 + retval;

        if flags & MSG_PEEK != 0 {
            //extend from the point after we read our previously peeked bytes
            interface::extend_fromptr_sized(newbufptr, retval as usize, &mut sockhandle.last_peek);
        }

        return totalbyteswritten;
    }

    fn recv_common_inner_udp(
        &self,
        sockhandle: &mut interface::RustLockWriteGuard<SocketHandle>,
        sockfdobj: &mut SocketDesc,
        buf: *mut u8,
        buflen: usize,
        addr: &mut Option<&mut interface::GenSockaddr>,
    ) -> i32 {
        let binddomain = if let Some(baddr) = addr {
            baddr.get_family() as i32
        } else {
            AF_INET
        };

        let ibindret = self._implicit_bind(&mut *sockhandle, binddomain);
        if ibindret < 0 {
            return ibindret;
        }

        loop {
            // loop for blocking sockets
            //if the remoteaddr is set and addr is not, use remoteaddr
            //unwrap is ok because of implicit bind
            let retval = if let (None, Some(ref mut remoteaddr)) = (&addr, sockhandle.remoteaddr) {
                sockhandle.innersocket.as_ref().unwrap().recvfrom(
                    buf,
                    buflen,
                    &mut Some(remoteaddr),
                )
            } else {
                sockhandle
                    .innersocket
                    .as_ref()
                    .unwrap()
                    .recvfrom(buf, buflen, addr)
            };

            if retval < 0 {
                match Errno::from_discriminant(interface::get_errno()) {
                    Ok(i) => {
                        if sockfdobj.flags & O_NONBLOCK == 0 && i == Errno::EAGAIN {
                            if self
                                .cancelstatus
                                .load(interface::RustAtomicOrdering::Relaxed)
                            {
                                // if the cancel status is set in the cage, we trap around a cancel point
                                // until the individual thread is signaled to cancel itself
                                loop {
                                    interface::cancelpoint(self.cageid);
                                }
                            }
                            interface::RustLockWriteGuard::<SocketHandle>::bump(sockhandle);
                            continue; //received EAGAIN on blocking socket, try again
                        }
                        return syscall_error(i, "recvfrom", "Internal call to recvfrom failed");
                    }
                    Err(()) => panic!("Unknown errno value from socket recvfrom returned!"),
                };
            } else {
                return retval; // we can proceed
            }
        }
    }

    pub fn recv_common(
        &self,
        fd: i32,
        buf: *mut u8,
        buflen: usize,
        flags: i32,
        addr: &mut Option<&mut interface::GenSockaddr>,
    ) -> i32 {
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let mut unlocked_fd = checkedfd.write();
        if let Some(ref mut filedesc_enum) = &mut *unlocked_fd {
            return self.recv_common_inner(filedesc_enum, buf, buflen, flags, addr);
        } else {
            return syscall_error(Errno::EBADF, "recvfrom", "invalid file descriptor");
        }
    }

    pub fn recvfrom_syscall(
        &self,
        fd: i32,
        buf: *mut u8,
        buflen: usize,
        flags: i32,
        addr: &mut Option<&mut interface::GenSockaddr>,
    ) -> i32 {
        return self.recv_common(fd, buf, buflen, flags, addr);
    }

    pub fn recv_syscall(&self, fd: i32, buf: *mut u8, buflen: usize, flags: i32) -> i32 {
        return self.recv_common(fd, buf, buflen, flags, &mut None);
    }

    //we currently ignore backlog
    pub fn listen_syscall(&self, fd: i32, _backlog: i32) -> i32 {
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let mut unlocked_fd = checkedfd.write();
        if let Some(filedesc_enum) = &mut *unlocked_fd {
            match filedesc_enum {
                Socket(ref mut sockfdobj) => {
                    //get or create the socket and bind it before listening
                    let sock_tmp = sockfdobj.handle.clone();
                    let mut sockhandle = sock_tmp.write();

                    match sockhandle.state {
                        ConnState::LISTEN => {
                            return 0; //Already done!
                        }

                        ConnState::CONNECTED
                        | ConnState::CONNRDONLY
                        | ConnState::CONNWRONLY
                        | ConnState::INPROGRESS => {
                            return syscall_error(
                                Errno::EOPNOTSUPP,
                                "listen",
                                "We don't support closing a prior socket connection on listen",
                            );
                        }

                        ConnState::NOTCONNECTED => {
                            if sockhandle.protocol != IPPROTO_TCP {
                                return syscall_error(
                                    Errno::EOPNOTSUPP,
                                    "listen",
                                    "This protocol doesn't support listening",
                                );
                            }

                            // simple if it's a domain socket
                            if sockhandle.domain == AF_UNIX {
                                sockhandle.state = ConnState::LISTEN;
                                return 0;
                            }

                            if sockhandle.localaddr.is_none() {
                                let shd = sockhandle.domain as i32;
                                let ibindret = self._implicit_bind(&mut *sockhandle, shd);
                                if ibindret < 0 {
                                    match Errno::from_discriminant(interface::get_errno()) {
                                        Ok(i) => {return syscall_error(i, "listen", "The libc call to bind within listen failed");},
                                        Err(()) => panic!("Unknown errno value from socket bind within listen returned!"),
                                    };
                                }
                            }

                            let ladr = sockhandle.localaddr.unwrap().clone(); //must have been populated by implicit bind
                            let porttuple = mux_port(
                                ladr.addr().clone(),
                                ladr.port(),
                                sockhandle.domain,
                                TCPPORT,
                            );

                            NET_METADATA.listening_port_set.insert(porttuple.clone());
                            sockhandle.state = ConnState::LISTEN;

                            let listenret = sockhandle.innersocket.as_ref().unwrap().listen(5); //default backlog in repy for whatever reason, we replicate it
                            if listenret < 0 {
                                let lr = match Errno::from_discriminant(interface::get_errno()) {
                                    Ok(i) => syscall_error(
                                        i,
                                        "listen",
                                        "The libc call to listen failed!",
                                    ),
                                    Err(()) => {
                                        panic!("Unknown errno value from socket listen returned!")
                                    }
                                };
                                NET_METADATA.listening_port_set.remove(&mux_port(
                                    ladr.addr().clone(),
                                    ladr.port(),
                                    sockhandle.domain,
                                    TCPPORT,
                                ));
                                sockhandle.state = ConnState::NOTCONNECTED;
                                return lr;
                            };

                            //set rawfd for select
                            sockfdobj.rawfd = sockhandle.innersocket.as_ref().unwrap().raw_sys_fd;

                            if !NET_METADATA.pending_conn_table.contains_key(&porttuple) {
                                NET_METADATA
                                    .pending_conn_table
                                    .insert(porttuple.clone(), vec![]);
                            }

                            return 0;
                        }
                    }
                }

                _ => {
                    return syscall_error(
                        Errno::ENOTSOCK,
                        "listen",
                        "file descriptor refers to something other than a socket",
                    );
                }
            }
        } else {
            return syscall_error(Errno::EBADF, "listen", "invalid file descriptor");
        }
    }

    pub fn netshutdown_syscall(&self, fd: i32, how: i32) -> i32 {
        match how {
            SHUT_RDWR | SHUT_RD | SHUT_WR => {
                return Self::_cleanup_socket(self, fd, how);
            }
            _ => {
                //See http://linux.die.net/man/2/shutdown for nuance to this error
                return syscall_error(
                    Errno::EINVAL,
                    "netshutdown",
                    "the shutdown how argument passed is not supported",
                );
            }
        }
    }

    pub fn _cleanup_socket_inner_helper(
        sockhandle: &mut SocketHandle,
        how: i32,
        shutdown: bool,
    ) -> i32 {
        // we need to do a bunch of actual socket cleanup for INET sockets
        if sockhandle.domain != AF_UNIX {
            let mut releaseflag = false;
            if let Some(ref sobj) = sockhandle.innersocket {
                if shutdown {
                    let shutresult = sobj.shutdown(how);

                    if shutresult < 0 {
                        match Errno::from_discriminant(interface::get_errno()) {
                            Ok(i) => {
                                return syscall_error(
                                    i,
                                    "shutdown",
                                    "The libc call to setsockopt failed!",
                                );
                            }
                            Err(()) => panic!("Unknown errno value from setsockopt returned!"),
                        };
                    }

                    match how {
                        SHUT_RD => {
                            if sockhandle.state == ConnState::CONNRDONLY {
                                releaseflag = true;
                            }
                        }
                        SHUT_WR => {
                            if sockhandle.state == ConnState::CONNWRONLY {
                                releaseflag = true;
                            }
                        }
                        SHUT_RDWR => {
                            releaseflag = true;
                        }
                        _ => {
                            //See http://linux.die.net/man/2/shutdown for nuance to this error
                            return syscall_error(
                                Errno::EINVAL,
                                "netshutdown",
                                "the shutdown how argument passed is not supported",
                            );
                        }
                    }
                } else {
                    //Reaching this means that the socket is closed. Removing the sockobj
                    //indicates that the sockobj will drop, and therefore close
                    releaseflag = true;
                    sockhandle.innersocket = None;
                }
            }

            if releaseflag {
                if let Some(localaddr) = sockhandle.localaddr.as_ref().clone() {
                    //move to end
                    let release_ret_val = NET_METADATA._release_localport(
                        localaddr.addr(),
                        localaddr.port(),
                        sockhandle.protocol,
                        sockhandle.domain,
                    );
                    sockhandle.localaddr = None;
                    if let Err(e) = release_ret_val {
                        return e;
                    }
                }
            }
        }

        // now change the connections for all socket types
        match how {
            SHUT_RD => {
                if sockhandle.state == ConnState::CONNWRONLY {
                    sockhandle.state = ConnState::NOTCONNECTED;
                } else {
                    sockhandle.state = ConnState::CONNWRONLY;
                }
            }
            SHUT_WR => {
                if sockhandle.state == ConnState::CONNRDONLY {
                    sockhandle.state = ConnState::NOTCONNECTED;
                } else {
                    sockhandle.state = ConnState::CONNRDONLY;
                }
            }
            SHUT_RDWR => {
                sockhandle.state = ConnState::NOTCONNECTED;
            }
            _ => {
                //See http://linux.die.net/man/2/shutdown for nuance to this error
                return syscall_error(
                    Errno::EINVAL,
                    "netshutdown",
                    "the shutdown how argument passed is not supported",
                );
            }
        }

        return 0;
    }

    pub fn _cleanup_socket_inner(
        &self,
        filedesc: &mut FileDescriptor,
        how: i32,
        shutdown: bool,
    ) -> i32 {
        if let Socket(sockfdobj) = filedesc {
            let sock_tmp = sockfdobj.handle.clone();
            let mut sockhandle = sock_tmp.write();

            Self::_cleanup_socket_inner_helper(&mut *sockhandle, how, shutdown)
        } else {
            syscall_error(
                Errno::ENOTSOCK,
                "cleanup socket",
                "file descriptor is not a socket",
            )
        }
    }

    pub fn _cleanup_socket(&self, fd: i32, how: i32) -> i32 {
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let mut unlocked_fd = checkedfd.write();
        if let Some(ref mut filedesc_enum) = &mut *unlocked_fd {
            let inner_result = self._cleanup_socket_inner(filedesc_enum, how, true);
            if inner_result < 0 {
                return inner_result;
            }

            if how == SHUT_RDWR {
                let _discarded_fd = unlocked_fd.take();
            }
        } else {
            return syscall_error(Errno::EBADF, "cleanup socket", "invalid file descriptor");
        }

        return 0;
    }

    pub fn accept_syscall(&self, fd: i32, addr: &mut interface::GenSockaddr) -> i32 {
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let mut unlocked_fd = checkedfd.write();
        if let Some(filedesc_enum) = &mut *unlocked_fd {
            let (newfd, guardopt) = self.get_next_fd(None);
            if newfd < 0 {
                return fd;
            }
            let newfdoption: &mut Option<FileDescriptor> = &mut *guardopt.unwrap();

            match filedesc_enum {
                Socket(ref mut sockfdobj) => {
                    let sock_tmp = sockfdobj.handle.clone();
                    let mut sockhandle = sock_tmp.read();

                    // check if domain socket
                    match sockhandle.domain {
                        AF_UNIX => {
                            return self.accept_unix(
                                &mut sockhandle,
                                sockfdobj,
                                newfd,
                                newfdoption,
                                addr,
                            )
                        }
                        AF_INET | AF_INET6 => {
                            return self.accept_inet(
                                &mut sockhandle,
                                sockfdobj,
                                newfd,
                                newfdoption,
                                addr,
                            )
                        }
                        _ => {
                            return syscall_error(
                                Errno::EINVAL,
                                "accept",
                                "Unsupported domain provided",
                            )
                        }
                    }
                }
                _ => {
                    return syscall_error(
                        Errno::ENOTSOCK,
                        "listen",
                        "file descriptor refers to something other than a socket",
                    );
                }
            }
        } else {
            return syscall_error(Errno::EBADF, "listen", "invalid file descriptor");
        }
    }

    fn accept_unix(
        &self,
        sockhandle: &mut interface::RustLockReadGuard<SocketHandle>,
        sockfdobj: &mut SocketDesc,
        newfd: i32,
        newfdoption: &mut Option<FileDescriptor>,
        addr: &mut interface::GenSockaddr,
    ) -> i32 {
        match sockhandle.protocol {
            IPPROTO_UDP => {
                return syscall_error(
                    Errno::EOPNOTSUPP,
                    "accept",
                    "Protocol does not support listening",
                );
            }
            IPPROTO_TCP => {
                if sockhandle.state != ConnState::LISTEN {
                    return syscall_error(
                        Errno::EINVAL,
                        "accept",
                        "Socket must be listening before accept is called",
                    );
                }
                let newsockfd = self._socket_initializer(
                    sockhandle.domain,
                    sockhandle.socktype,
                    sockhandle.protocol,
                    sockfdobj.flags & O_NONBLOCK != 0,
                    sockfdobj.flags & O_CLOEXEC != 0,
                    ConnState::CONNECTED,
                );

                let remote_addr: interface::GenSockaddr;
                let sendpipenumber;
                let receivepipenumber;

                loop {
                    let localpathbuf =
                        normpath(convpath(sockhandle.localaddr.unwrap().path()), self);
                    let dsconnobj = NET_METADATA.domsock_accept_table.get(&localpathbuf);

                    if let Some(ds) = dsconnobj {
                        // we loop here to accept the connection
                        // if we get a connection object from the accept table, we complete the connection and set up the address and pipes
                        // if theres no object, we retry, except in the case of non-blocking accept where we return EAGAIN
                        if let Some(connvar) = ds.get_cond_var() {
                            if !connvar.broadcast() {
                                drop(ds);
                                continue;
                            }
                        }
                        let addr = ds.get_sockaddr().clone();
                        remote_addr = addr.clone();
                        receivepipenumber = ds.get_receive_pipe().clone();
                        sendpipenumber = ds.get_send_pipe().clone();
                        drop(ds);
                        NET_METADATA.domsock_accept_table.remove(&localpathbuf);
                        break;
                    } else {
                        if 0 != (sockfdobj.flags & O_NONBLOCK) {
                            // if non block return EAGAIN
                            return syscall_error(
                                Errno::EAGAIN,
                                "accept",
                                "host system accept call failed",
                            );
                        }
                    }
                }

                let newsock_tmp = newsockfd.handle.clone();
                let mut newsockhandle = newsock_tmp.write();

                let pathclone = normpath(convpath(remote_addr.path()), self);
                if let Some(inodenum) = metawalk(pathclone.as_path()) {
                    newsockhandle.unix_info = Some(UnixSocketInfo {
                        inode: inodenum.clone(),
                        mode: sockhandle.unix_info.as_ref().unwrap().mode,
                        sendpipe: Some(sendpipenumber.clone()),
                        receivepipe: Some(receivepipenumber.clone()),
                    });
                    if let Inode::Socket(ref mut sock) =
                        *(FS_METADATA.inodetable.get_mut(&inodenum).unwrap())
                    {
                        sock.refcount += 1;
                    }
                };

                newsockhandle.localaddr = Some(sockhandle.localaddr.unwrap().clone());
                newsockhandle.remoteaddr = Some(remote_addr.clone());
                newsockhandle.state = ConnState::CONNECTED;

                let _insertval = newfdoption.insert(Socket(newsockfd));
                *addr = remote_addr; //populate addr with what address it connected to

                return newfd;
            }
            _ => {
                return syscall_error(Errno::EOPNOTSUPP, "accept", "Unkown protocol in accept");
            }
        }
    }

    fn accept_inet(
        &self,
        sockhandle: &mut interface::RustLockReadGuard<SocketHandle>,
        sockfdobj: &mut SocketDesc,
        newfd: i32,
        newfdoption: &mut Option<FileDescriptor>,
        addr: &mut interface::GenSockaddr,
    ) -> i32 {
        match sockhandle.protocol {
            IPPROTO_UDP => {
                return syscall_error(
                    Errno::EOPNOTSUPP,
                    "accept",
                    "Protocol does not support listening",
                );
            }
            IPPROTO_TCP => {
                if sockhandle.state != ConnState::LISTEN {
                    return syscall_error(
                        Errno::EINVAL,
                        "accept",
                        "Socket must be listening before accept is called",
                    );
                }
                let mut newsockfd = self._socket_initializer(
                    sockhandle.domain,
                    sockhandle.socktype,
                    sockhandle.protocol,
                    sockfdobj.flags & O_NONBLOCK != 0,
                    sockfdobj.flags & O_CLOEXEC != 0,
                    ConnState::CONNECTED,
                );

                loop {
                    // we loop here so we can cancel blocking accept, see comments below and in Socket::new in interface/comm.rs

                    // if we got a pending connection in select/poll/whatever, return that here instead
                    let ladr = sockhandle.localaddr.unwrap().clone(); //must have been populated by implicit bind
                    let porttuple =
                        mux_port(ladr.addr().clone(), ladr.port(), sockhandle.domain, TCPPORT);

                    let mut pendingvec =
                        NET_METADATA.pending_conn_table.get_mut(&porttuple).unwrap();
                    let pendingoption = pendingvec.pop();
                    let (acceptedresult, remote_addr) = match pendingoption {
                        Some(pendingtup) => pendingtup,
                        None => {
                            //unwrap ok because listening
                            if 0 == (sockfdobj.flags & O_NONBLOCK) {
                                match sockhandle.domain {
                                    PF_INET => {
                                        sockhandle.innersocket.as_ref().unwrap().accept(true)
                                    }
                                    PF_INET6 => {
                                        sockhandle.innersocket.as_ref().unwrap().accept(false)
                                    }
                                    _ => panic!("Unknown domain in accepting socket"),
                                }
                            } else {
                                match sockhandle.domain {
                                    PF_INET => sockhandle
                                        .innersocket
                                        .as_ref()
                                        .unwrap()
                                        .nonblock_accept(true),
                                    PF_INET6 => sockhandle
                                        .innersocket
                                        .as_ref()
                                        .unwrap()
                                        .nonblock_accept(false),
                                    _ => panic!("Unknown domain in accepting socket"),
                                }
                            }
                        }
                    };

                    if let Err(_) = acceptedresult {
                        match Errno::from_discriminant(interface::get_errno()) {
                            Ok(i) => {
                                //We have the socket timeout set to every one second, so
                                //if our blocking socket ever returns EAGAIN, it must be
                                //the case that this recv timeout was exceeded, and we
                                //should thus not treat this as a failure in our emulated
                                //socket; see comment in Socket::new in interface/comm.rs
                                if sockfdobj.flags & O_NONBLOCK == 0 && i == Errno::EAGAIN {
                                    if self
                                        .cancelstatus
                                        .load(interface::RustAtomicOrdering::Relaxed)
                                    {
                                        // if the cancel status is set in the cage, we trap around a cancel point
                                        // until the individual thread is signaled to cancel itself
                                        loop {
                                            interface::cancelpoint(self.cageid);
                                        }
                                    }
                                    continue; // EAGAIN, try again
                                }

                                return syscall_error(
                                    i,
                                    "accept",
                                    "Internal call to accept failed",
                                );
                            }
                            Err(()) => panic!("Unknown errno value from socket accept returned!"),
                        };
                    }

                    // if we get here we have an accepted socket
                    let acceptedsock = acceptedresult.unwrap();

                    let mut newaddr = sockhandle.localaddr.unwrap().clone();
                    let newport = match NET_METADATA._reserve_localport(
                        newaddr.addr(),
                        0,
                        sockhandle.protocol,
                        sockhandle.domain,
                        false,
                    ) {
                        Ok(portnum) => portnum,
                        Err(errnum) => {
                            return errnum;
                        }
                    };
                    newaddr.set_port(newport);

                    let newsock_tmp = newsockfd.handle.clone();
                    let mut newsockhandle = newsock_tmp.write();

                    newsockhandle.localaddr = Some(newaddr);
                    newsockhandle.remoteaddr = Some(remote_addr.clone());

                    //create socket object for new connected socket
                    newsockhandle.innersocket = Some(acceptedsock);
                    // set lock-free rawfd for select
                    newsockfd.rawfd = newsockhandle.innersocket.as_ref().unwrap().raw_sys_fd;

                    let _insertval = newfdoption.insert(Socket(newsockfd));
                    *addr = remote_addr; //populate addr with what address it connected to

                    return newfd;
                }
            }
            _ => {
                return syscall_error(Errno::EOPNOTSUPP, "accept", "Unkown protocol in accept");
            }
        }
    }

    pub fn select_syscall(
        &self,
        nfds: i32,
        readfds: Option<&mut interface::FdSet>,
        writefds: Option<&mut interface::FdSet>,
        exceptfds: Option<&mut interface::FdSet>,
        timeout: Option<interface::RustDuration>,
    ) -> i32 {
        if nfds < STARTINGFD || nfds >= FD_SET_MAX_FD {
            return syscall_error(Errno::EINVAL, "select", "Number of FDs is wrong");
        }

        let start_time = interface::starttimer();

        let end_time = match timeout {
            Some(time) => time,
            None => interface::RustDuration::MAX,
        };

        let mut retval = 0;
        // in the loop below, we always read from original fd_sets, but make updates to the new copies
        let new_readfds = &mut interface::FdSet::new();
        let new_writefds = &mut interface::FdSet::new();
        loop {
            //we must block manually
            // 1. iterate thru readfds
            if let Some(readfds_ref) = readfds.as_ref() {
                let res = self.select_readfds(nfds, readfds_ref, new_readfds, &mut retval);
                if res != 0 {
                    return res;
                }
            }

            // 2. iterate thru writefds
            if let Some(writefds_ref) = writefds.as_ref() {
                let res = self.select_writefds(nfds, writefds_ref, new_writefds, &mut retval);
                if res != 0 {
                    return res;
                }
            }

            // 3. iterate thru exceptfds
            // currently we don't really do select on execptfds, we just check if those fds are valid
            if let Some(exceptfds_ref) = exceptfds.as_ref() {
                for fd in 0..nfds {
                    // find the bit and see if it's on
                    if !exceptfds_ref.is_set(fd) {
                        continue;
                    }
                    let checkedfd = self.get_filedescriptor(fd).unwrap();
                    let unlocked_fd = checkedfd.read();
                    if unlocked_fd.is_none() {
                        return syscall_error(Errno::EBADF, "select", "invalid file descriptor");
                    }
                }
            }

            if retval != 0 || interface::readtimer(start_time) > end_time {
                break;
            } else {
                // at this point lets check if we got a signal before sleeping
                if interface::sigcheck() {
                    return syscall_error(Errno::EINTR, "select", "interrupted function call");
                }
                interface::lind_yield();
            }
        }

        // Now we copy our internal FdSet struct results back into the *mut libc::fd_set
        if readfds.is_some() {
            readfds.unwrap().copy_from(&new_readfds);
        }

        if writefds.is_some() {
            writefds.unwrap().copy_from(&new_writefds);
        }

        return retval;
    }

    fn select_readfds(
        &self,
        nfds: i32,
        readfds: &interface::FdSet,
        new_readfds: &mut interface::FdSet,
        retval: &mut i32,
    ) -> i32 {
        // For INET: prepare the data structures for the kernel_select's use
        let mut inet_info = SelectInetInfo::new();

        for fd in 0..nfds {
            // check if current i is in readfd
            if !readfds.is_set(fd) {
                continue;
            }

            let checkedfd = self.get_filedescriptor(fd).unwrap();
            let unlocked_fd = checkedfd.read();
            if let Some(filedesc_enum) = &*unlocked_fd {
                match filedesc_enum {
                    Socket(ref sockfdobj) => {
                        let mut newconnection = false;
                        match sockfdobj.domain {
                            AF_UNIX => {
                                let sock_tmp = sockfdobj.handle.clone();
                                let sockhandle = sock_tmp.read();
                                if sockhandle.state == ConnState::INPROGRESS {
                                    let remotepathbuf = normpath(
                                        convpath(sockhandle.remoteaddr.unwrap().path()),
                                        self,
                                    );
                                    let dsconnobj =
                                        NET_METADATA.domsock_accept_table.get(&remotepathbuf);
                                    if dsconnobj.is_none() {
                                        newconnection = true;
                                    }
                                }

                                if sockhandle.state == ConnState::LISTEN {
                                    let localpathbuf = normpath(
                                        convpath(sockhandle.localaddr.unwrap().path()),
                                        self,
                                    );
                                    let dsconnobj =
                                        NET_METADATA.domsock_accept_table.get(&localpathbuf);
                                    if dsconnobj.is_some() {
                                        // we have a connecting domain socket, return as readable to be accepted
                                        new_readfds.set(fd);
                                        *retval += 1;
                                    }
                                } else if sockhandle.state == ConnState::CONNECTED || newconnection
                                {
                                    let sockinfo = &sockhandle.unix_info.as_ref().unwrap();
                                    let receivepipe = sockinfo.receivepipe.as_ref().unwrap();
                                    if receivepipe.check_select_read() {
                                        new_readfds.set(fd);
                                        *retval += 1;
                                    }
                                }
                            }
                            AF_INET | AF_INET6 => {
                                // here we simply record the inet fd into inet_fds and the tuple list for using kernel_select
                                if sockfdobj.rawfd < 0 {
                                    continue;
                                }

                                inet_info.kernel_fds.set(sockfdobj.rawfd);
                                inet_info.rawfd_lindfd_tuples.push((sockfdobj.rawfd, fd));
                                if sockfdobj.rawfd > inet_info.highest_raw_fd {
                                    inet_info.highest_raw_fd = sockfdobj.rawfd;
                                }
                            }
                            _ => {
                                return syscall_error(
                                    Errno::EINVAL,
                                    "select",
                                    "Unsupported domain provided",
                                )
                            }
                        }

                        if newconnection {
                            let sock_tmp = sockfdobj.handle.clone();
                            let mut sockhandle = sock_tmp.write();
                            sockhandle.state = ConnState::CONNECTED;
                        }
                    }

                    //we don't support selecting streams
                    Stream(_) => {
                        continue;
                    }

                    Pipe(pipefdobj) => {
                        if pipefdobj.pipe.check_select_read() {
                            new_readfds.set(fd);
                            *retval += 1;
                        }
                    }

                    //these file reads never block
                    _ => {
                        new_readfds.set(fd);
                        *retval += 1;
                    }
                }
            } else {
                return syscall_error(Errno::EBADF, "select", "invalid file descriptor");
            }
        }

        // do the kernel_select for inet sockets
        if !inet_info.kernel_fds.is_empty() {
            let kernel_ret = update_readfds_from_kernel_select(new_readfds, &mut inet_info, retval);
            // NOTE: we ignore the kernel_select error if some domsocks are ready
            if kernel_ret < 0 && *retval <= 0 {
                return kernel_ret;
            }
        }

        return 0;
    }

    fn select_writefds(
        &self,
        nfds: i32,
        writefds: &interface::FdSet,
        new_writefds: &mut interface::FdSet,
        retval: &mut i32,
    ) -> i32 {
        for fd in 0..nfds {
            // check if current i is in writefds
            if !writefds.is_set(fd) {
                continue;
            }

            let checkedfd = self.get_filedescriptor(fd).unwrap();
            let unlocked_fd = checkedfd.read();
            if let Some(filedesc_enum) = &*unlocked_fd {
                match filedesc_enum {
                    Socket(ref sockfdobj) => {
                        // check if we've made an in progress connection first
                        let sock_tmp = sockfdobj.handle.clone();
                        let sockhandle = sock_tmp.read();
                        let mut newconnection = false;
                        match sockhandle.domain {
                            AF_UNIX => {
                                if sockhandle.state == ConnState::INPROGRESS {
                                    let remotepathbuf =
                                        convpath(sockhandle.remoteaddr.unwrap().path());
                                    let dsconnobj =
                                        NET_METADATA.domsock_accept_table.get(&remotepathbuf);
                                    if dsconnobj.is_none() {
                                        newconnection = true;
                                    }
                                }
                            }
                            AF_INET => {
                                if sockhandle.state == ConnState::INPROGRESS
                                    && sockhandle
                                        .innersocket
                                        .as_ref()
                                        .unwrap()
                                        .check_rawconnection()
                                {
                                    newconnection = true;
                                }
                            }
                            _ => {
                                return syscall_error(Errno::EINVAL, "select", "Unsupported domain")
                            }
                        }

                        if newconnection {
                            let mut newconnhandle = sock_tmp.write();
                            newconnhandle.state = ConnState::CONNECTED;
                        }

                        //we always say sockets are writable? Even though this is not true
                        new_writefds.set(fd);
                        *retval += 1;
                    }

                    //we always say streams are writable?
                    Stream(_) => {
                        new_writefds.set(fd);
                        *retval += 1;
                    }

                    Pipe(pipefdobj) => {
                        if pipefdobj.pipe.check_select_write() {
                            new_writefds.set(fd);
                            *retval += 1;
                        }
                    }

                    //these file writes never block
                    _ => {
                        new_writefds.set(fd);
                        *retval += 1;
                    }
                }
            } else {
                return syscall_error(Errno::EBADF, "select", "invalid file descriptor");
            }
        }
        return 0;
    }

    pub fn getsockopt_syscall(&self, fd: i32, level: i32, optname: i32, optval: &mut i32) -> i32 {
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let mut unlocked_fd = checkedfd.write();
        if let Some(filedesc_enum) = &mut *unlocked_fd {
            if let Socket(ref mut sockfdobj) = filedesc_enum {
                let optbit = 1 << optname;
                let sock_tmp = sockfdobj.handle.clone();
                let mut sockhandle = sock_tmp.write();
                match level {
                    SOL_UDP => {
                        return syscall_error(
                            Errno::EOPNOTSUPP,
                            "getsockopt",
                            "UDP is not supported for getsockopt",
                        );
                    }
                    SOL_TCP => {
                        // Checking the tcp_options here
                        // Currently only support TCP_NODELAY option for SOL_TCP
                        if optname == TCP_NODELAY {
                            let optbit = 1 << optname;
                            if optbit & sockhandle.tcp_options == optbit {
                                *optval = 1;
                            } else {
                                *optval = 0;
                            }
                            return 0;
                        }
                        return syscall_error(
                            Errno::EOPNOTSUPP,
                            "getsockopt",
                            "TCP options not remembered by getsockopt",
                        );
                    }
                    SOL_SOCKET => {
                        // checking the socket_options here
                        match optname {
                            //indicate whether we are accepting connections or not in the moment
                            SO_ACCEPTCONN => {
                                if sockhandle.state == ConnState::LISTEN {
                                    *optval = 1;
                                } else {
                                    *optval = 0;
                                }
                            }
                            //if the option is a stored binary option, just return it...
                            SO_LINGER | SO_KEEPALIVE | SO_SNDLOWAT | SO_RCVLOWAT | SO_REUSEPORT
                            | SO_REUSEADDR => {
                                if sockhandle.socket_options & optbit == optbit {
                                    *optval = 1;
                                } else {
                                    *optval = 0;
                                }
                            }
                            //handling the ignored buffer settings:
                            SO_SNDBUF => {
                                *optval = sockhandle.sndbuf;
                            }
                            SO_RCVBUF => {
                                *optval = sockhandle.rcvbuf;
                            }
                            //returning the type if asked
                            SO_TYPE => {
                                *optval = sockhandle.socktype;
                            }
                            //should always be true
                            SO_OOBINLINE => {
                                *optval = 1;
                            }
                            SO_ERROR => {
                                let tmp = sockhandle.errno;
                                sockhandle.errno = 0;
                                *optval = tmp;
                            }
                            _ => {
                                return syscall_error(
                                    Errno::EOPNOTSUPP,
                                    "getsockopt",
                                    "unknown optname passed into syscall",
                                );
                            }
                        }
                    }
                    _ => {
                        return syscall_error(
                            Errno::EOPNOTSUPP,
                            "getsockopt",
                            "unknown level passed into syscall",
                        );
                    }
                }
            } else {
                return syscall_error(
                    Errno::ENOTSOCK,
                    "getsockopt",
                    "the provided file descriptor is not a socket",
                );
            }
        } else {
            return syscall_error(
                Errno::EBADF,
                "getsockopt",
                "the provided file descriptor is invalid",
            );
        }
        return 0;
    }

    pub fn setsockopt_syscall(&self, fd: i32, level: i32, optname: i32, optval: i32) -> i32 {
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let mut unlocked_fd = checkedfd.write();
        if let Some(filedesc_enum) = &mut *unlocked_fd {
            if let Socket(ref mut sockfdobj) = filedesc_enum {
                //checking that we recieved SOL_SOCKET
                match level {
                    SOL_UDP => {
                        return syscall_error(
                            Errno::EOPNOTSUPP,
                            "setsockopt",
                            "UDP is not supported for getsockopt",
                        );
                    }
                    SOL_TCP => {
                        // Here we check and set tcp_options
                        // Currently only support TCP_NODELAY for SOL_TCP
                        if optname == TCP_NODELAY {
                            let optbit = 1 << optname;
                            let sock_tmp = sockfdobj.handle.clone();
                            let mut sockhandle = sock_tmp.write();
                            let mut newoptions = sockhandle.tcp_options;
                            //now let's set this if we were told to
                            if optval != 0 {
                                //optval should always be 1 or 0.
                                newoptions |= optbit;
                            } else {
                                newoptions &= !optbit;
                            }

                            if newoptions != sockhandle.tcp_options {
                                if let Some(sock) = sockhandle.innersocket.as_ref() {
                                    let sockret = sock.setsockopt(SOL_TCP, optname, optval);
                                    if sockret < 0 {
                                        match Errno::from_discriminant(interface::get_errno()) {
                                            Ok(i) => {
                                                return syscall_error(
                                                    i,
                                                    "setsockopt",
                                                    "The libc call to setsockopt failed!",
                                                );
                                            }
                                            Err(()) => panic!(
                                                "Unknown errno value from setsockopt returned!"
                                            ),
                                        };
                                    }
                                }
                            }
                            sockhandle.tcp_options = newoptions;
                            return 0;
                        }
                        return syscall_error(
                            Errno::EOPNOTSUPP,
                            "setsockopt",
                            "This TCP option is not remembered by setsockopt",
                        );
                    }
                    SOL_SOCKET => {
                        // Here we check and set socket_options
                        let optbit = 1 << optname;
                        let sock_tmp = sockfdobj.handle.clone();
                        let mut sockhandle = sock_tmp.write();

                        match optname {
                            SO_ACCEPTCONN | SO_TYPE | SO_SNDLOWAT | SO_RCVLOWAT => {
                                let error_string =
                                    format!("Cannot set option using setsockopt. {}", optname);
                                return syscall_error(
                                    Errno::ENOPROTOOPT,
                                    "setsockopt",
                                    &error_string,
                                );
                            }
                            SO_LINGER | SO_KEEPALIVE => {
                                if optval == 0 {
                                    sockhandle.socket_options &= !optbit;
                                } else {
                                    //optval should always be 1 or 0.
                                    sockhandle.socket_options |= optbit;
                                }

                                return 0;
                            }

                            SO_REUSEPORT | SO_REUSEADDR => {
                                let mut newoptions = sockhandle.socket_options;
                                //now let's set this if we were told to
                                if optval != 0 {
                                    //optval should always be 1 or 0.
                                    newoptions |= optbit;
                                } else {
                                    newoptions &= !optbit;
                                }

                                if newoptions != sockhandle.socket_options {
                                    if let Some(sock) = sockhandle.innersocket.as_ref() {
                                        let sockret = sock.setsockopt(SOL_SOCKET, optname, optval);
                                        if sockret < 0 {
                                            match Errno::from_discriminant(interface::get_errno()) {
                                                Ok(i) => {
                                                    return syscall_error(
                                                        i,
                                                        "setsockopt",
                                                        "The libc call to setsockopt failed!",
                                                    );
                                                }
                                                Err(()) => panic!(
                                                    "Unknown errno value from setsockopt returned!"
                                                ),
                                            };
                                        }
                                    }
                                }

                                sockhandle.socket_options = newoptions;

                                return 0;
                            }
                            SO_SNDBUF => {
                                sockhandle.sndbuf = optval;
                                return 0;
                            }
                            SO_RCVBUF => {
                                sockhandle.rcvbuf = optval;
                                return 0;
                            }
                            //should always be one -- can only handle it being 1
                            SO_OOBINLINE => {
                                if optval != 1 {
                                    return syscall_error(
                                        Errno::EOPNOTSUPP,
                                        "getsockopt",
                                        "does not support OOBINLINE being set to anything but 1",
                                    );
                                }
                                return 0;
                            }
                            _ => {
                                return syscall_error(
                                    Errno::EOPNOTSUPP,
                                    "getsockopt",
                                    "unknown optname passed into syscall",
                                );
                            }
                        }
                    }
                    _ => {
                        return syscall_error(
                            Errno::EOPNOTSUPP,
                            "getsockopt",
                            "unknown level passed into syscall",
                        );
                    }
                }
            } else {
                return syscall_error(
                    Errno::ENOTSOCK,
                    "getsockopt",
                    "the provided file descriptor is not a socket",
                );
            }
        } else {
            return syscall_error(
                Errno::EBADF,
                "getsockopt",
                "the provided file descriptor is invalid",
            );
        }
    }

    pub fn getpeername_syscall(&self, fd: i32, ret_addr: &mut interface::GenSockaddr) -> i32 {
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let unlocked_fd = checkedfd.read();
        if let Some(filedesc_enum) = &*unlocked_fd {
            if let Socket(sockfdobj) = filedesc_enum {
                //if the socket is not connected, then we should return an error
                let sock_tmp = sockfdobj.handle.clone();
                let sockhandle = sock_tmp.read();
                if sockhandle.remoteaddr == None {
                    return syscall_error(
                        Errno::ENOTCONN,
                        "getpeername",
                        "the socket is not connected",
                    );
                }
                *ret_addr = sockhandle.remoteaddr.unwrap();
                return 0;
            } else {
                return syscall_error(
                    Errno::ENOTSOCK,
                    "getpeername",
                    "the provided file is not a socket",
                );
            }
        } else {
            return syscall_error(
                Errno::EBADF,
                "getpeername",
                "the provided file descriptor is not valid",
            );
        }
    }

    pub fn getsockname_syscall(&self, fd: i32, ret_addr: &mut interface::GenSockaddr) -> i32 {
        let checkedfd = self.get_filedescriptor(fd).unwrap();
        let unlocked_fd = checkedfd.read();
        if let Some(filedesc_enum) = &*unlocked_fd {
            if let Socket(sockfdobj) = filedesc_enum {
                let sock_tmp = sockfdobj.handle.clone();
                let sockhandle = sock_tmp.read();
                if sockhandle.domain == AF_UNIX {
                    if sockhandle.localaddr == None {
                        let null_path: &[u8] = &[];
                        *ret_addr = interface::GenSockaddr::Unix(interface::new_sockaddr_unix(
                            sockhandle.domain as u16,
                            null_path,
                        ));
                        return 0;
                    }
                    //if the socket is not none, then return the socket
                    *ret_addr = sockhandle.localaddr.unwrap();
                    return 0;
                } else {
                    if sockhandle.localaddr == None {
                        //sets the address to 0.0.0.0 if the address is not initialized yet
                        //setting the family as well based on the domain
                        let addr = match sockhandle.domain {
                            AF_INET => interface::GenIpaddr::V4(interface::V4Addr::default()),
                            AF_INET6 => interface::GenIpaddr::V6(interface::V6Addr::default()),
                            _ => {
                                unreachable!()
                            }
                        };
                        ret_addr.set_addr(addr);
                        ret_addr.set_port(0);
                        ret_addr.set_family(sockhandle.domain as u16);
                        return 0;
                    }
                    *ret_addr = sockhandle.localaddr.unwrap();
                    return 0;
                }
            } else {
                return syscall_error(
                    Errno::ENOTSOCK,
                    "getsockname",
                    "the provided file is not a socket",
                );
            }
        } else {
            return syscall_error(
                Errno::EBADF,
                "getsockname",
                "the provided file descriptor is not valid",
            );
        }
    }

    //we only return the default host name because we do not allow for the user to change the host name right now
    pub fn gethostname_syscall(&self, address_ptr: *mut u8, length: isize) -> i32 {
        if length < 0 {
            return syscall_error(
                Errno::EINVAL,
                "gethostname_syscall",
                "provided length argument is invalid",
            );
        }

        let mut bytes: Vec<u8> = DEFAULT_HOSTNAME.as_bytes().to_vec();
        bytes.push(0u8); //Adding a null terminator to the end of the string
        let name_length = bytes.len();

        let mut len = name_length;
        if (length as usize) < len {
            len = length as usize;
        }

        interface::fill(address_ptr, len, &bytes);

        return 0;
    }

    pub fn poll_syscall(
        &self,
        fds: &mut [PollStruct],
        timeout: Option<interface::RustDuration>,
    ) -> i32 {
        //timeout is supposed to be in milliseconds

        let mut return_code: i32 = 0;
        let start_time = interface::starttimer();

        let end_time = match timeout {
            Some(time) => time,
            None => interface::RustDuration::MAX,
        };

        loop {
            for structpoll in &mut *fds {
                let fd = structpoll.fd;
                let events = structpoll.events;

                // init FdSet structures
                let reads = &mut interface::FdSet::new();
                let writes = &mut interface::FdSet::new();
                let errors = &mut interface::FdSet::new();

                //read
                if events & POLLIN > 0 {
                    reads.set(fd)
                }
                //write
                if events & POLLOUT > 0 {
                    writes.set(fd)
                }
                //err
                if events & POLLERR > 0 {
                    errors.set(fd)
                }

                let mut mask: i16 = 0;

                //0 essentially sets the timeout to the max value allowed (which is almost always more than enough time)
                // NOTE that the nfds argument is highest fd + 1
                let selectret = Self::select_syscall(
                    &self,
                    fd + 1,
                    Some(reads),
                    Some(writes),
                    Some(errors),
                    Some(interface::RustDuration::ZERO),
                );
                if selectret > 0 {
                    mask += if !reads.is_empty() { POLLIN } else { 0 };
                    mask += if !writes.is_empty() { POLLOUT } else { 0 };
                    mask += if !errors.is_empty() { POLLERR } else { 0 };
                    return_code += 1;
                } else if selectret < 0 {
                    return selectret;
                }
                structpoll.revents = mask;
            }

            if return_code != 0 || interface::readtimer(start_time) > end_time {
                break;
            } else {
                if interface::sigcheck() {
                    return syscall_error(Errno::EINTR, "poll", "interrupted function call");
                }
                interface::lind_yield();
            }
        }
        return return_code;
    }

    pub fn _epoll_object_allocator(&self) -> i32 {
        //seems to only be called in functions that don't have a filedesctable lock, so not passing the lock.

        let epollobjfd = Epoll(EpollDesc {
            mode: 0000,
            registered_fds: interface::RustHashMap::<i32, EpollEvent>::new(),
            advlock: interface::RustRfc::new(interface::AdvisoryLock::new()),
            errno: 0,
            flags: 0,
        });
        //get a file descriptor
        let (fd, guardopt) = self.get_next_fd(None);
        if fd < 0 {
            return fd;
        }
        let fdoption = &mut *guardopt.unwrap();
        let _insertval = fdoption.insert(epollobjfd);

        return fd;
    }

    pub fn epoll_create_syscall(&self, size: i32) -> i32 {
        if size <= 0 {
            return syscall_error(
                Errno::EINVAL,
                "epoll create",
                "provided size argument is invalid",
            );
        }
        return Self::_epoll_object_allocator(self);
    }

    //this one can still be optimized
    pub fn epoll_ctl_syscall(&self, epfd: i32, op: i32, fd: i32, event: &EpollEvent) -> i32 {
        //making sure that the epfd is really an epoll fd
        let checkedfd = self.get_filedescriptor(epfd).unwrap();
        let mut unlocked_fd = checkedfd.write();
        if let Some(filedesc_enum_epollfd) = &mut *unlocked_fd {
            if let Epoll(epollfdobj) = filedesc_enum_epollfd {
                //check if the other fd is an epoll or not...
                let checkedfd = self.get_filedescriptor(fd).unwrap();
                let unlocked_fd = checkedfd.read();
                if let Some(filedesc_enum) = &*unlocked_fd {
                    if let Epoll(_) = filedesc_enum {
                        return syscall_error(
                            Errno::EBADF,
                            "epoll ctl",
                            "provided fd is not a valid file descriptor",
                        );
                    }
                } else {
                    return syscall_error(
                        Errno::EBADF,
                        "epoll ctl",
                        "provided fd is not a valid file descriptor",
                    );
                }

                //now that we know that the types are all good...
                match op {
                    EPOLL_CTL_DEL => {
                        //since remove returns the value at the key and the values will always be EpollEvents,
                        //I am using this to optimize the code
                        epollfdobj.registered_fds.remove(&fd).unwrap().1;
                    }
                    EPOLL_CTL_MOD => {
                        //check if the fd that we are modifying exists or not
                        if !epollfdobj.registered_fds.contains_key(&fd) {
                            return syscall_error(
                                Errno::ENOENT,
                                "epoll ctl",
                                "fd is not registered with this epfd",
                            );
                        }
                        //if the fd already exists, insert overwrites the prev entry
                        epollfdobj.registered_fds.insert(
                            fd,
                            EpollEvent {
                                events: event.events,
                                fd: event.fd,
                            },
                        );
                    }
                    EPOLL_CTL_ADD => {
                        if epollfdobj.registered_fds.contains_key(&fd) {
                            return syscall_error(
                                Errno::EEXIST,
                                "epoll ctl",
                                "fd is already registered",
                            );
                        }
                        epollfdobj.registered_fds.insert(
                            fd,
                            EpollEvent {
                                events: event.events,
                                fd: event.fd,
                            },
                        );
                    }
                    _ => {
                        return syscall_error(Errno::EINVAL, "epoll ctl", "provided op is invalid");
                    }
                }
            } else {
                return syscall_error(
                    Errno::EBADF,
                    "epoll ctl",
                    "provided fd is not a valid file descriptor",
                );
            }
        } else {
            return syscall_error(
                Errno::EBADF,
                "epoll ctl",
                "provided epoll fd is not a valid epoll file descriptor",
            );
        }
        return 0;
    }

    pub fn epoll_wait_syscall(
        &self,
        epfd: i32,
        events: &mut [EpollEvent],
        maxevents: i32,
        timeout: Option<interface::RustDuration>,
    ) -> i32 {
        let checkedfd = self.get_filedescriptor(epfd).unwrap();
        let mut unlocked_fd = checkedfd.write();
        if let Some(filedesc_enum) = &mut *unlocked_fd {
            if let Epoll(epollfdobj) = filedesc_enum {
                if maxevents < 0 {
                    return syscall_error(
                        Errno::EINVAL,
                        "epoll wait",
                        "max events argument is not a positive number",
                    );
                }
                let mut poll_fds_vec: Vec<PollStruct> = vec![];
                let mut rm_fds_vec: Vec<i32> = vec![];
                let mut num_events: usize = 0;
                for set in epollfdobj.registered_fds.iter() {
                    let (&key, &value) = set.pair();

                    // check if any of the registered fds were closed, add them to remove list
                    let checkedregfd = self.get_filedescriptor(key).unwrap();
                    let unlocked_regfd = checkedregfd.read();
                    if unlocked_regfd.is_none() {
                        rm_fds_vec.push(key);
                        continue;
                    }

                    let events = value.events;
                    let mut structpoll = PollStruct {
                        fd: key,
                        events: 0,
                        revents: 0,
                    };
                    if events & EPOLLIN as u32 > 0 {
                        structpoll.events |= POLLIN;
                    }
                    if events & EPOLLOUT as u32 > 0 {
                        structpoll.events |= POLLOUT;
                    }
                    if events & EPOLLERR as u32 > 0 {
                        structpoll.events |= POLLERR;
                    }
                    poll_fds_vec.push(structpoll);
                    num_events += 1;
                }

                for fd in rm_fds_vec.iter() {
                    epollfdobj.registered_fds.remove(fd);
                } // remove closed fds

                let poll_fds_slice = &mut poll_fds_vec[..];
                let pollret = Self::poll_syscall(&self, poll_fds_slice, timeout);
                if pollret < 0 {
                    return pollret;
                }
                let mut count = 0;
                let end_idx: usize = interface::rust_min(num_events, maxevents as usize);
                for result in poll_fds_slice[..end_idx].iter() {
                    let mut poll_event = false;
                    let mut event = EpollEvent {
                        events: 0,
                        fd: epollfdobj.registered_fds.get(&result.fd).unwrap().fd,
                    };
                    if result.revents & POLLIN > 0 {
                        event.events |= EPOLLIN as u32;
                        poll_event = true;
                    }
                    if result.revents & POLLOUT > 0 {
                        event.events |= EPOLLOUT as u32;
                        poll_event = true;
                    }
                    if result.revents & POLLERR > 0 {
                        event.events |= EPOLLERR as u32;
                        poll_event = true;
                    }

                    if poll_event {
                        events[count] = event;
                        count += 1;
                    }
                }
                return count as i32;
            } else {
                return syscall_error(
                    Errno::EINVAL,
                    "epoll wait",
                    "provided fd is not an epoll file descriptor",
                );
            }
        } else {
            return syscall_error(
                Errno::EBADF,
                "epoll wait",
                "provided fd is not a valid file descriptor",
            );
        }
    }

    // Because socketpair needs to spawn off a helper thread to connect the two ends of the socket pair, and because that helper thread,
    // along with the main thread, need to access the cage to call methods (syscalls) of it, and because rust's threading model states that
    // any reference passed into a thread but not moved into it mut have a static lifetime, we cannot use a standard member function to perform
    // this syscall, and must use an arc wrapped cage instead as a "this" parameter in lieu of self
    pub fn socketpair_syscall(
        this: interface::RustRfc<Cage>,
        domain: i32,
        socktype: i32,
        protocol: i32,
        sv: &mut interface::SockPair,
    ) -> i32 {
        let newprotocol = if protocol == 0 { IPPROTO_TCP } else { protocol };
        // firstly check the parameters
        if domain != AF_UNIX {
            return syscall_error(
                Errno::EOPNOTSUPP,
                "socketpair",
                "Linux socketpair only supports AF_UNIX aka AF_LOCAL domain.",
            );
        } else if socktype & 0x7 != SOCK_STREAM || newprotocol != IPPROTO_TCP {
            return syscall_error(
                Errno::EOPNOTSUPP,
                "socketpair",
                "Socketpair currently only supports SOCK_STREAM TCP.",
            );
        }

        let nonblocking = (socktype & SOCK_NONBLOCK) != 0;
        let cloexec = (socktype & SOCK_CLOEXEC) != 0;

        // create 2 file discriptors
        let sock1fdobj = this._socket_initializer(
            domain,
            socktype,
            newprotocol,
            nonblocking,
            cloexec,
            ConnState::NOTCONNECTED,
        );
        let sock1fd = this._socket_inserter(Socket(sock1fdobj.clone()));
        let sock2fdobj = this._socket_initializer(
            domain,
            socktype,
            newprotocol,
            nonblocking,
            cloexec,
            ConnState::NOTCONNECTED,
        );
        let sock2fd = this._socket_inserter(Socket(sock2fdobj.clone()));

        // assign local addresses and connect
        let sock1tmp = sock1fdobj.handle.clone();
        let sock2tmp = sock2fdobj.handle.clone();
        let mut sock1handle = sock1tmp.write();
        let mut sock2handle = sock2tmp.write();
        let localaddr1 = Self::assign_new_addr_unix(&sock1handle);
        let localaddr2 = Self::assign_new_addr_unix(&sock2handle);
        this.bind_inner_socket(&mut *sock1handle, &localaddr1, false);
        this.bind_inner_socket(&mut *sock2handle, &localaddr2, false);

        // setup the pipes
        let (pipe1, pipe2) = create_unix_sockpipes();
        // one handle's remote address is the other's local address
        sock1handle.remoteaddr = Some(localaddr2.clone());
        sock2handle.remoteaddr = Some(localaddr1.clone());
        // one handle's sendpipe is the other's receivepipe
        sock1handle.unix_info.as_mut().unwrap().sendpipe = Some(pipe1.clone());
        sock1handle.unix_info.as_mut().unwrap().receivepipe = Some(pipe2.clone());
        sock2handle.unix_info.as_mut().unwrap().sendpipe = Some(pipe2.clone());
        sock2handle.unix_info.as_mut().unwrap().receivepipe = Some(pipe1.clone());

        // now they are connected
        sock1handle.state = ConnState::CONNECTED;
        sock2handle.state = ConnState::CONNECTED;

        sv.sock1 = sock1fd;
        sv.sock2 = sock2fd;

        // we need to increment the refcount of the sockets we created
        // reason: in bind_inner_socket, we added entries to the inode table
        let inode1num = sock1handle.unix_info.as_mut().unwrap().inode;
        if let Inode::Socket(ref mut sock) = *(FS_METADATA.inodetable.get_mut(&inode1num).unwrap())
        {
            sock.refcount += 1;
        }
        let inode2num = sock2handle.unix_info.as_mut().unwrap().inode;
        if let Inode::Socket(ref mut sock) = *(FS_METADATA.inodetable.get_mut(&inode2num).unwrap())
        {
            sock.refcount += 1;
        }

        return 0;
    }

    // all this does is send the net_devs data in a string to libc, where we will later parse and
    // alloc into getifaddrs structs
    pub fn getifaddrs_syscall(&self, buf: *mut u8, count: usize) -> i32 {
        if NET_IFADDRS_STR.len() < count {
            interface::fill(
                buf,
                NET_IFADDRS_STR.len(),
                &NET_IFADDRS_STR.as_bytes().to_vec(),
            );
            0 // return success
        } else {
            return syscall_error(Errno::EOPNOTSUPP, "getifaddrs", "invalid ifaddrs length");
        }
    }
}
