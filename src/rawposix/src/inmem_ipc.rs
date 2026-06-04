use fdtables::FDTableEntry;
use libc::{c_void, linger, sockaddr, sockaddr_in, sockaddr_storage, sockaddr_un, socklen_t};
use once_cell::sync::Lazy;
use parking_lot::Mutex as ParkingMutex;
use ringbuf::{Consumer, Producer, RingBuffer};
use std::cmp::min;
use std::collections::{HashMap, VecDeque};
use std::ffi::CStr;
use std::mem;
use std::ptr;
use std::slice;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, Weak};
use sysdefs::constants::err_const::{syscall_error, Errno};
use sysdefs::constants::fs_const::O_NONBLOCK;
use sysdefs::constants::lind_platform_const::{FDKIND_IMPIPE, FDKIND_IMSOCK};

const PIPE_CAPACITY: usize = 65_536;
const UDSOCK_CAPACITY: usize = 212_992;
const PAGE_SIZE: usize = 4096;
const CANCEL_CHECK_INTERVAL: usize = 1_048_576;
const EPHEMERAL_PORT_RANGE_START: u16 = 32_768;
const EPHEMERAL_PORT_RANGE_END: u16 = 60_999;
const LOOPBACK_ADDR: u32 = 0x7f00_0007;

static NEXT_ENDPOINT_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_SOCKET_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_EPHEMERAL_PORT: AtomicU64 = AtomicU64::new(EPHEMERAL_PORT_RANGE_END as u64);

static ENDPOINTS: Lazy<Mutex<HashMap<u64, Arc<PipeEndpoint>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static SOCKETS: Lazy<Mutex<HashMap<u64, Arc<InmemSocket>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static LISTENERS: Lazy<Mutex<HashMap<String, Arc<InmemSocket>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Copy, PartialEq, Eq)]
enum PipeSide {
    Read,
    Write,
}

struct PipeEndpoint {
    queue: Arc<ByteQueue>,
    side: PipeSide,
}

struct ByteQueue {
    write_end: Arc<ParkingMutex<Producer<u8>>>,
    read_end: Arc<ParkingMutex<Consumer<u8>>>,
    refcount_write: Arc<AtomicU32>,
    refcount_read: Arc<AtomicU32>,
    eof: Arc<AtomicBool>,
    capacity: usize,
}

impl ByteQueue {
    fn new(capacity: usize) -> Self {
        let rb = RingBuffer::<u8>::new(capacity);
        let (prod, cons) = rb.split();
        Self {
            write_end: Arc::new(ParkingMutex::new(prod)),
            read_end: Arc::new(ParkingMutex::new(cons)),
            refcount_write: Arc::new(AtomicU32::new(1)),
            refcount_read: Arc::new(AtomicU32::new(1)),
            eof: Arc::new(AtomicBool::new(false)),
            capacity,
        }
    }

    fn get_read_ref(&self) -> u32 {
        self.refcount_read.load(Ordering::Relaxed)
    }

    fn get_write_ref(&self) -> u32 {
        self.refcount_write.load(Ordering::Relaxed)
    }

    fn close_readers(&self) {
        self.refcount_read.store(0, Ordering::Relaxed);
    }

    fn close_writers(&self) {
        self.refcount_write.store(0, Ordering::Relaxed);
        self.eof.store(true, Ordering::Relaxed);
    }

    fn read(&self, dst: *mut u8, count: usize, nonblocking: bool) -> i32 {
        if count == 0 {
            return 0;
        }

        let buf = unsafe {
            assert!(!dst.is_null());
            slice::from_raw_parts_mut(dst, count)
        };

        let mut read_end = self.read_end.lock();
        let mut pipe_space = read_end.len();
        if nonblocking && pipe_space == 0 {
            if self.eof.load(Ordering::SeqCst) {
                return 0;
            }
            return syscall_error(Errno::EAGAIN, "read", "would block");
        }

        let mut checks = 0usize;
        while pipe_space == 0 {
            if self.eof.load(Ordering::SeqCst) || self.get_write_ref() == 0 {
                return 0;
            }

            if checks == CANCEL_CHECK_INTERVAL {
                return -(Errno::EAGAIN as i32);
            }

            pipe_space = read_end.len();
            checks += 1;
            if pipe_space == 0 {
                std::thread::yield_now();
            }
        }

        let bytes_to_read = min(count, pipe_space);
        read_end.pop_slice(&mut buf[..bytes_to_read]);
        bytes_to_read as i32
    }

    fn write(&self, src: *const u8, count: usize, nonblocking: bool) -> i32 {
        self.write_with_limit(src, count, nonblocking, self.capacity)
    }

    fn write_with_limit(
        &self,
        src: *const u8,
        count: usize,
        nonblocking: bool,
        limit: usize,
    ) -> i32 {
        if count == 0 {
            return 0;
        }

        let buf = unsafe {
            assert!(!src.is_null());
            slice::from_raw_parts(src, count)
        };

        let mut bytes_written = 0usize;
        let mut write_end = self.write_end.lock();
        let limit = limit.clamp(1, self.capacity);

        let queued = self.capacity - write_end.remaining();
        if nonblocking && queued >= limit {
            return syscall_error(Errno::EAGAIN, "write", "would block");
        }

        while bytes_written < count {
            if self.get_read_ref() == 0 {
                return syscall_error(Errno::EPIPE, "write", "broken pipe");
            }

            let queued = self.capacity - write_end.remaining();
            let remaining = write_end.remaining().min(limit.saturating_sub(queued));
            if remaining == 0 {
                if nonblocking {
                    if bytes_written > 0 {
                        return bytes_written as i32;
                    }
                    return syscall_error(Errno::EAGAIN, "write", "would block");
                }
                std::thread::yield_now();
                continue;
            }

            if !nonblocking
                && remaining != self.capacity
                && (count - bytes_written) > PAGE_SIZE
                && remaining < PAGE_SIZE
            {
                std::thread::yield_now();
                continue;
            }

            let bytes_to_write = min(count, bytes_written + remaining);
            write_end.push_slice(&buf[bytes_written..bytes_to_write]);
            bytes_written = bytes_to_write;
        }

        bytes_written as i32
    }

    fn poll_read(&self) -> i16 {
        let has_data = self.read_end.lock().len() > 0;
        if has_data || self.eof.load(Ordering::SeqCst) || self.get_write_ref() == 0 {
            libc::POLLIN as i16
        } else {
            0
        }
    }

    fn poll_write(&self) -> i16 {
        if self.get_read_ref() == 0 {
            return libc::POLLERR as i16;
        }
        if self.write_end.lock().remaining() != 0 {
            libc::POLLOUT as i16
        } else {
            0
        }
    }
}

struct InmemSocket {
    incoming: Arc<ByteQueue>,
    state: Mutex<SocketState>,
    accept_ready: Condvar,
}

struct SocketState {
    domain: i32,
    socktype: i32,
    protocol: i32,
    local_key: Option<String>,
    peer: Option<Weak<InmemSocket>>,
    listening: bool,
    closed: bool,
    read_shutdown: bool,
    write_shutdown: bool,
    reuseaddr: i32,
    reuseport: i32,
    sndbuf: i32,
    rcvbuf: i32,
    linger: linger,
    tcp_nodelay: i32,
    pending: VecDeque<Arc<InmemSocket>>,
}

impl InmemSocket {
    fn new(domain: i32, socktype: i32, protocol: i32) -> Self {
        let capacity = socket_capacity(domain);
        Self {
            incoming: Arc::new(ByteQueue::new(capacity)),
            state: Mutex::new(SocketState {
                domain,
                socktype,
                protocol,
                local_key: None,
                peer: None,
                listening: false,
                closed: false,
                read_shutdown: false,
                write_shutdown: false,
                reuseaddr: 0,
                reuseport: 0,
                sndbuf: capacity as i32,
                rcvbuf: capacity as i32,
                linger: linger {
                    l_onoff: 0,
                    l_linger: 0,
                },
                tcp_nodelay: 0,
                pending: VecDeque::new(),
            }),
            accept_ready: Condvar::new(),
        }
    }
}

fn socket_capacity(domain: i32) -> usize {
    if domain == libc::AF_UNIX || domain == libc::AF_INET {
        UDSOCK_CAPACITY
    } else {
        PIPE_CAPACITY
    }
}

pub fn fd_is_nonblocking(fdentry: FDTableEntry) -> bool {
    (fdentry.perfdinfo as i32 & O_NONBLOCK) != 0
}

pub fn create_pipe() -> (u64, u64) {
    let queue = Arc::new(ByteQueue::new(PIPE_CAPACITY));
    let read_id = NEXT_ENDPOINT_ID.fetch_add(1, Ordering::Relaxed);
    let write_id = NEXT_ENDPOINT_ID.fetch_add(1, Ordering::Relaxed);

    let mut endpoints = ENDPOINTS.lock().unwrap();
    endpoints.insert(
        read_id,
        Arc::new(PipeEndpoint {
            queue: queue.clone(),
            side: PipeSide::Read,
        }),
    );
    endpoints.insert(
        write_id,
        Arc::new(PipeEndpoint {
            queue,
            side: PipeSide::Write,
        }),
    );

    (read_id, write_id)
}

pub fn create_socket(domain: i32, socktype: i32, protocol: i32) -> u64 {
    register_socket(Arc::new(InmemSocket::new(domain, socktype, protocol)))
}

fn register_socket(socket: Arc<InmemSocket>) -> u64 {
    let id = NEXT_SOCKET_ID.fetch_add(1, Ordering::Relaxed);
    SOCKETS.lock().unwrap().insert(id, socket);
    id
}

fn get_socket(socket_id: u64) -> Result<Arc<InmemSocket>, i32> {
    SOCKETS
        .lock()
        .unwrap()
        .get(&socket_id)
        .cloned()
        .ok_or_else(|| syscall_error(Errno::EBADF, "inmem_socket", "bad socket"))
}

fn get_endpoint(endpoint_id: u64) -> Result<Arc<PipeEndpoint>, i32> {
    ENDPOINTS
        .lock()
        .unwrap()
        .get(&endpoint_id)
        .cloned()
        .ok_or_else(|| syscall_error(Errno::EBADF, "inmem_pipe", "bad pipe"))
}

fn read_with_mode(
    fdentry: FDTableEntry,
    dst: *mut u8,
    count: usize,
    force_nonblocking: bool,
) -> i32 {
    let nonblocking = force_nonblocking || fd_is_nonblocking(fdentry);

    match fdentry.fdkind {
        FDKIND_IMPIPE => match get_endpoint(fdentry.underfd) {
            Ok(endpoint) if endpoint.side == PipeSide::Read => {
                endpoint.queue.read(dst, count, nonblocking)
            }
            Ok(_) => syscall_error(Errno::EBADF, "read", "pipe is not readable"),
            Err(e) => e,
        },
        FDKIND_IMSOCK => match get_socket(fdentry.underfd) {
            Ok(socket) => {
                if socket.state.lock().unwrap().read_shutdown {
                    return 0;
                }
                socket.incoming.read(dst, count, nonblocking)
            }
            Err(e) => e,
        },
        _ => syscall_error(Errno::EBADF, "read", "unsupported in-memory fd"),
    }
}

pub fn read(fdentry: FDTableEntry, dst: *mut u8, count: usize) -> i32 {
    read_with_mode(fdentry, dst, count, false)
}

fn write_with_mode(
    fdentry: FDTableEntry,
    src: *const u8,
    count: usize,
    force_nonblocking: bool,
) -> i32 {
    let nonblocking = force_nonblocking || fd_is_nonblocking(fdentry);

    match fdentry.fdkind {
        FDKIND_IMPIPE => match get_endpoint(fdentry.underfd) {
            Ok(endpoint) if endpoint.side == PipeSide::Write => {
                endpoint.queue.write(src, count, nonblocking)
            }
            Ok(_) => syscall_error(Errno::EBADF, "write", "pipe is not writable"),
            Err(e) => e,
        },
        FDKIND_IMSOCK => match get_socket(fdentry.underfd) {
            Ok(socket) => {
                let (peer, sndbuf) = {
                    let state = socket.state.lock().unwrap();
                    if state.write_shutdown {
                        return syscall_error(Errno::EPIPE, "write", "socket write shut down");
                    }
                    (
                        state.peer.as_ref().and_then(Weak::upgrade),
                        state.sndbuf.max(1) as usize,
                    )
                };
                match peer {
                    Some(peer) => peer
                        .incoming
                        .write_with_limit(src, count, nonblocking, sndbuf),
                    None => syscall_error(Errno::EPIPE, "write", "socket peer closed"),
                }
            }
            Err(e) => e,
        },
        _ => syscall_error(Errno::EBADF, "write", "unsupported in-memory fd"),
    }
}

pub fn write(fdentry: FDTableEntry, src: *const u8, count: usize) -> i32 {
    write_with_mode(fdentry, src, count, false)
}

pub fn close_fd(fdentry: FDTableEntry, _count: u64) -> Result<(), i32> {
    match fdentry.fdkind {
        FDKIND_IMPIPE => {
            if let Ok(endpoint) = get_endpoint(fdentry.underfd) {
                match endpoint.side {
                    PipeSide::Read => endpoint.queue.close_readers(),
                    PipeSide::Write => endpoint.queue.close_writers(),
                }
            }
        }
        FDKIND_IMSOCK => {
            if let Ok(socket) = get_socket(fdentry.underfd) {
                close_socket(&socket);
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn unix_path_is_bound(path: &str) -> bool {
    let mut key = String::from("unix:");
    key.push_str(path);
    if LISTENERS.lock().unwrap().contains_key(&key) {
        return true;
    }

    SOCKETS.lock().unwrap().values().any(|socket| {
        let state = socket.state.lock().unwrap();
        state.closed == false && state.local_key.as_deref() == Some(key.as_str())
    })
}

fn close_socket(socket: &Arc<InmemSocket>) {
    let (key, peer) = {
        let mut state = socket.state.lock().unwrap();
        if state.closed {
            return;
        }
        state.closed = true;
        (
            state.local_key.clone(),
            state.peer.as_ref().and_then(Weak::upgrade),
        )
    };

    if let Some(key) = key {
        let mut listeners = LISTENERS.lock().unwrap();
        if listeners
            .get(&key)
            .is_some_and(|listener| Arc::ptr_eq(listener, socket))
        {
            listeners.remove(&key);
        }
    }

    socket.incoming.close_readers();
    if let Some(peer) = peer {
        peer.incoming.close_writers();
        let mut peer_state = peer.state.lock().unwrap();
        peer_state.peer = None;
    }
}

pub fn shutdown_socket(socket_id: u64, how: i32) -> i32 {
    let socket = match get_socket(socket_id) {
        Ok(socket) => socket,
        Err(e) => return e,
    };

    let peer = {
        let mut state = socket.state.lock().unwrap();
        match how {
            libc::SHUT_RD => state.read_shutdown = true,
            libc::SHUT_WR => state.write_shutdown = true,
            libc::SHUT_RDWR => {
                state.read_shutdown = true;
                state.write_shutdown = true;
            }
            _ => return syscall_error(Errno::EINVAL, "shutdown", "invalid shutdown mode"),
        }
        state.peer.as_ref().and_then(Weak::upgrade)
    };

    if how == libc::SHUT_RD || how == libc::SHUT_RDWR {
        socket.incoming.close_readers();
    }
    if how == libc::SHUT_WR || how == libc::SHUT_RDWR {
        if let Some(peer) = peer {
            peer.incoming.close_writers();
        }
    }

    0
}

pub fn bind_socket(socket_id: u64, key: String) -> i32 {
    let socket = match get_socket(socket_id) {
        Ok(socket) => socket,
        Err(e) => return e,
    };

    let key = {
        let state = socket.state.lock().unwrap();
        if key == "inet:0" {
            next_ephemeral_key(state.domain)
        } else {
            key
        }
    };

    if LISTENERS.lock().unwrap().contains_key(&key) {
        return syscall_error(Errno::EADDRINUSE, "bind", "address already in use");
    }
    if SOCKETS.lock().unwrap().values().any(|other| {
        if Arc::ptr_eq(other, &socket) {
            return false;
        }
        let state = other.state.lock().unwrap();
        !state.closed
            && state
                .local_key
                .as_ref()
                .is_some_and(|other_key| other_key == &key)
    }) {
        return syscall_error(Errno::EADDRINUSE, "bind", "address already in use");
    }

    let mut state = socket.state.lock().unwrap();
    state.local_key = Some(key);
    0
}

pub fn listen_socket(socket_id: u64, backlog: i32) -> i32 {
    let socket = match get_socket(socket_id) {
        Ok(socket) => socket,
        Err(e) => return e,
    };

    let key = {
        let mut state = socket.state.lock().unwrap();
        if state.local_key.is_none() {
            let key = next_ephemeral_key(state.domain);
            state.local_key = Some(key);
        }
        state.listening = true;
        state.local_key.clone().unwrap()
    };

    let mut listeners = LISTENERS.lock().unwrap();
    if let Some(listener) = listeners.get(&key) {
        if Arc::ptr_eq(listener, &socket) {
            return 0;
        }
        return syscall_error(Errno::EADDRINUSE, "listen", "address already in use");
    }
    listeners.insert(key, socket);
    let _ = backlog;
    0
}

pub fn connect_socket(socket_id: u64, remote_key: String) -> i32 {
    let client = match get_socket(socket_id) {
        Ok(socket) => socket,
        Err(e) => return e,
    };
    let listener = match LISTENERS.lock().unwrap().get(&remote_key).cloned() {
        Some(listener) => listener,
        None => return syscall_error(Errno::ECONNREFUSED, "connect", "no listener"),
    };

    let (domain, socktype, protocol) = {
        let state = client.state.lock().unwrap();
        (state.domain, state.socktype, state.protocol)
    };

    let server_conn = Arc::new(InmemSocket::new(domain, socktype, protocol));

    {
        let mut client_state = client.state.lock().unwrap();
        if client_state.local_key.is_none() {
            client_state.local_key = Some(next_ephemeral_key(client_state.domain));
        }
        client_state.peer = Some(Arc::downgrade(&server_conn));
    }
    {
        let mut server_state = server_conn.state.lock().unwrap();
        server_state.local_key = Some(remote_key);
        server_state.peer = Some(Arc::downgrade(&client));
    }
    {
        let mut listener_state = listener.state.lock().unwrap();
        if !listener_state.listening {
            return syscall_error(Errno::ECONNREFUSED, "connect", "socket is not listening");
        }
        listener_state.pending.push_back(server_conn);
        listener.accept_ready.notify_all();
    }

    0
}

pub fn accept_socket(fdentry: FDTableEntry) -> Result<u64, i32> {
    let listener = get_socket(fdentry.underfd)?;
    let nonblocking = fd_is_nonblocking(fdentry);
    let mut state = listener.state.lock().unwrap();

    loop {
        if let Some(conn) = state.pending.pop_front() {
            return Ok(register_socket(conn));
        }
        if !state.listening || state.closed {
            return Err(syscall_error(Errno::EINVAL, "accept", "not listening"));
        }
        if nonblocking {
            return Err(syscall_error(Errno::EAGAIN, "accept", "would block"));
        }
        state = listener.accept_ready.wait(state).unwrap();
    }
}

pub fn socketpair(domain: i32, socktype: i32, protocol: i32) -> (u64, u64) {
    let s1 = Arc::new(InmemSocket::new(domain, socktype, protocol));
    let s2 = Arc::new(InmemSocket::new(domain, socktype, protocol));
    {
        s1.state.lock().unwrap().peer = Some(Arc::downgrade(&s2));
        s2.state.lock().unwrap().peer = Some(Arc::downgrade(&s1));
    }
    (register_socket(s1), register_socket(s2))
}

pub fn sockaddr_key(addr: *mut u8, addrlen: socklen_t) -> Option<(i32, String)> {
    if addr.is_null() || addrlen < mem::size_of::<libc::sa_family_t>() as socklen_t {
        return None;
    }

    let family = unsafe { (*(addr as *const sockaddr)).sa_family as i32 };
    match family {
        libc::AF_UNIX => unix_sockaddr_key(addr, addrlen).map(|key| (family, key)),
        libc::AF_INET => inet_sockaddr_key(addr).map(|key| (family, key)),
        _ => None,
    }
}

pub fn is_supported_socket(domain: i32, socktype: i32, protocol: i32) -> bool {
    let base_type = socktype & !(libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK);
    base_type == libc::SOCK_STREAM
        && (domain == libc::AF_UNIX || domain == libc::AF_INET)
        && (protocol == 0 || protocol == libc::IPPROTO_TCP)
}

pub fn sendto(fdentry: FDTableEntry, buf: *const c_void, buflen: usize, flags: i32) -> i32 {
    let force_nonblocking = (flags & libc::MSG_DONTWAIT) != 0;
    write_with_mode(fdentry, buf as *const u8, buflen, force_nonblocking)
}

pub fn recvfrom(fdentry: FDTableEntry, buf: *mut c_void, buflen: usize, flags: i32) -> i32 {
    let force_nonblocking = (flags & libc::MSG_DONTWAIT) != 0;
    read_with_mode(fdentry, buf as *mut u8, buflen, force_nonblocking)
}

fn sockaddr_from_key(
    domain: i32,
    key: Option<&str>,
    opname: &str,
) -> Result<(sockaddr_storage, socklen_t), i32> {
    let mut storage: sockaddr_storage = unsafe { mem::zeroed() };

    match domain {
        libc::AF_INET => {
            let sockaddr = unsafe { &mut *(&mut storage as *mut _ as *mut sockaddr_in) };
            sockaddr.sin_family = libc::AF_INET as libc::sa_family_t;
            sockaddr.sin_addr.s_addr = u32::to_be(0x7f000001);
            sockaddr.sin_port = key
                .and_then(|key| key.strip_prefix("inet:"))
                .and_then(|port| port.parse::<u16>().ok())
                .map(u16::to_be)
                .unwrap_or(0);
            Ok((storage, mem::size_of::<sockaddr_in>() as socklen_t))
        }
        libc::AF_UNIX => {
            let sockaddr = unsafe { &mut *(&mut storage as *mut _ as *mut sockaddr_un) };
            sockaddr.sun_family = libc::AF_UNIX as libc::sa_family_t;
            if let Some(path) = key.and_then(|key| key.strip_prefix("unix:")) {
                let bytes = path.as_bytes();
                let n = bytes.len().min(sockaddr.sun_path.len().saturating_sub(1));
                for (idx, byte) in bytes.iter().take(n).enumerate() {
                    sockaddr.sun_path[idx] = *byte as libc::c_char;
                }
            }
            Ok((storage, mem::size_of::<sockaddr_un>() as socklen_t))
        }
        _ => Err(syscall_error(
            Errno::EOPNOTSUPP,
            opname,
            "unsupported in-memory socket family",
        )),
    }
}

pub fn getsockname(socket_id: u64) -> Result<(sockaddr_storage, socklen_t), i32> {
    let socket = get_socket(socket_id)?;
    let state = socket.state.lock().unwrap();
    sockaddr_from_key(state.domain, state.local_key.as_deref(), "getsockname")
}

pub fn getpeername(socket_id: u64) -> Result<(sockaddr_storage, socklen_t), i32> {
    let socket = get_socket(socket_id)?;
    let (domain, peer) = {
        let state = socket.state.lock().unwrap();
        (state.domain, state.peer.as_ref().and_then(Weak::upgrade))
    };

    let peer = match peer {
        Some(peer) => peer,
        None => {
            return Err(syscall_error(
                Errno::ENOTCONN,
                "getpeername",
                "socket is not connected",
            ))
        }
    };
    let peer_state = peer.state.lock().unwrap();
    sockaddr_from_key(domain, peer_state.local_key.as_deref(), "getpeername")
}

pub fn setsockopt(
    socket_id: u64,
    level: i32,
    optname: i32,
    optval: *const c_void,
    optlen: socklen_t,
) -> i32 {
    if optval.is_null() {
        return syscall_error(Errno::EFAULT, "setsockopt", "option value is null");
    }

    let socket = match get_socket(socket_id) {
        Ok(socket) => socket,
        Err(e) => return e,
    };
    let mut state = socket.state.lock().unwrap();

    match (level, optname) {
        (libc::SOL_SOCKET, libc::SO_REUSEADDR) if optlen as usize >= mem::size_of::<i32>() => {
            state.reuseaddr = unsafe { *(optval as *const i32) };
            0
        }
        (libc::SOL_SOCKET, libc::SO_REUSEPORT) if optlen as usize >= mem::size_of::<i32>() => {
            state.reuseport = unsafe { *(optval as *const i32) };
            0
        }
        (libc::SOL_SOCKET, libc::SO_SNDBUF) if optlen as usize >= mem::size_of::<i32>() => {
            let requested = unsafe { *(optval as *const i32) }.max(1);
            state.sndbuf = requested.saturating_mul(2).saturating_sub(128).max(1);
            0
        }
        (libc::SOL_SOCKET, libc::SO_RCVBUF) if optlen as usize >= mem::size_of::<i32>() => {
            state.rcvbuf = unsafe { *(optval as *const i32) }.max(1);
            0
        }
        (libc::SOL_SOCKET, libc::SO_LINGER) if optlen as usize >= mem::size_of::<linger>() => {
            state.linger = unsafe { *(optval as *const linger) };
            0
        }
        (libc::IPPROTO_TCP, libc::TCP_NODELAY) if optlen as usize >= mem::size_of::<i32>() => {
            state.tcp_nodelay = unsafe { *(optval as *const i32) };
            0
        }
        _ => syscall_error(
            Errno::ENOPROTOOPT,
            "setsockopt",
            "unsupported socket option",
        ),
    }
}

pub fn getsockopt(
    socket_id: u64,
    level: i32,
    optname: i32,
    optval: *mut c_void,
    optlen: *mut socklen_t,
) -> i32 {
    if optval.is_null() || optlen.is_null() {
        return syscall_error(Errno::EFAULT, "getsockopt", "option pointer is null");
    }

    let socket = match get_socket(socket_id) {
        Ok(socket) => socket,
        Err(e) => return e,
    };
    let state = socket.state.lock().unwrap();

    unsafe fn write_opt<T: Copy>(optval: *mut c_void, optlen: *mut socklen_t, value: T) -> i32 {
        let required = mem::size_of::<T>() as socklen_t;
        if *optlen < required {
            return syscall_error(Errno::EINVAL, "getsockopt", "option buffer too small");
        }
        *(optval as *mut T) = value;
        *optlen = required;
        0
    }

    match (level, optname) {
        (libc::SOL_SOCKET, libc::SO_TYPE) => unsafe {
            write_opt(
                optval,
                optlen,
                state.socktype & !(libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK),
            )
        },
        (libc::SOL_SOCKET, libc::SO_ERROR) => unsafe { write_opt(optval, optlen, 0i32) },
        (libc::SOL_SOCKET, libc::SO_ACCEPTCONN) => unsafe {
            write_opt(optval, optlen, if state.listening { 1i32 } else { 0i32 })
        },
        (libc::SOL_SOCKET, libc::SO_REUSEADDR) => unsafe {
            write_opt(optval, optlen, state.reuseaddr)
        },
        (libc::SOL_SOCKET, libc::SO_REUSEPORT) => unsafe {
            write_opt(optval, optlen, state.reuseport)
        },
        (libc::SOL_SOCKET, libc::SO_SNDBUF) => unsafe { write_opt(optval, optlen, state.sndbuf) },
        (libc::SOL_SOCKET, libc::SO_RCVBUF) => unsafe { write_opt(optval, optlen, state.rcvbuf) },
        (libc::SOL_SOCKET, libc::SO_LINGER) => unsafe { write_opt(optval, optlen, state.linger) },
        (libc::IPPROTO_TCP, libc::TCP_NODELAY) => unsafe {
            write_opt(optval, optlen, state.tcp_nodelay)
        },
        _ => syscall_error(
            Errno::ENOPROTOOPT,
            "getsockopt",
            "unsupported socket option",
        ),
    }
}

pub fn poll_fd(fdentry: FDTableEntry, events: i16) -> i16 {
    let mut revents = 0i16;
    match fdentry.fdkind {
        FDKIND_IMPIPE => {
            if let Ok(endpoint) = get_endpoint(fdentry.underfd) {
                match endpoint.side {
                    PipeSide::Read => {
                        if events & libc::POLLIN as i16 != 0 {
                            revents |= endpoint.queue.poll_read();
                        }
                    }
                    PipeSide::Write => {
                        if events & libc::POLLOUT as i16 != 0 {
                            revents |= endpoint.queue.poll_write();
                        }
                    }
                }
            } else {
                revents |= libc::POLLNVAL as i16;
            }
        }
        FDKIND_IMSOCK => {
            if let Ok(socket) = get_socket(fdentry.underfd) {
                if events & libc::POLLIN as i16 != 0 {
                    let listening_ready = {
                        let state = socket.state.lock().unwrap();
                        if state.read_shutdown {
                            revents |= libc::POLLIN as i16;
                            return revents;
                        }
                        state.listening && !state.pending.is_empty()
                    };
                    if listening_ready {
                        revents |= libc::POLLIN as i16;
                    } else {
                        revents |= socket.incoming.poll_read();
                    }
                }
                if events & libc::POLLOUT as i16 != 0 {
                    let peer = {
                        let state = socket.state.lock().unwrap();
                        if state.write_shutdown {
                            revents |= libc::POLLERR as i16;
                            return revents;
                        }
                        state.peer.as_ref().and_then(Weak::upgrade)
                    };
                    match peer {
                        Some(peer) => revents |= peer.incoming.poll_write(),
                        None => revents |= libc::POLLERR as i16,
                    }
                }
            } else {
                revents |= libc::POLLNVAL as i16;
            }
        }
        _ => revents |= libc::POLLNVAL as i16,
    }
    revents
}

fn unix_sockaddr_key(addr: *mut u8, addrlen: socklen_t) -> Option<String> {
    if addrlen < mem::size_of::<libc::sa_family_t>() as socklen_t {
        return None;
    }

    let sockaddr = unsafe { &*(addr as *const sockaddr_un) };
    let path_len = (addrlen as usize)
        .saturating_sub(mem::size_of::<libc::sa_family_t>())
        .min(sockaddr.sun_path.len());

    if path_len == 0 {
        return None;
    }

    let bytes =
        unsafe { std::slice::from_raw_parts(sockaddr.sun_path.as_ptr() as *const u8, path_len) };
    if bytes.first() == Some(&0) {
        return Some(format!("unix:@{}", String::from_utf8_lossy(&bytes[1..])));
    }

    let cstr = unsafe { CStr::from_ptr(sockaddr.sun_path.as_ptr()) };
    Some(format!("unix:{}", cstr.to_string_lossy()))
}

fn inet_sockaddr_key(addr: *mut u8) -> Option<String> {
    let sockaddr = unsafe { &*(addr as *const sockaddr_in) };
    let ip = u32::from_be(sockaddr.sin_addr.s_addr);
    if ip != LOOPBACK_ADDR && ip != 0 {
        return None;
    }

    let port = u16::from_be(sockaddr.sin_port);
    Some(format!("unix:tmp/loopback{}", port))
}

fn next_ephemeral_key(domain: i32) -> String {
    match domain {
        libc::AF_INET => {
            let port = NEXT_EPHEMERAL_PORT
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                    let next = if current <= EPHEMERAL_PORT_RANGE_START as u64 {
                        EPHEMERAL_PORT_RANGE_END as u64
                    } else {
                        current - 1
                    };
                    Some(next)
                })
                .unwrap_or(EPHEMERAL_PORT_RANGE_END as u64) as u16;
            format!("inet:{}", port)
        }
        _ => {
            let id = NEXT_SOCKET_ID.load(Ordering::Relaxed);
            format!("unix:@autobind{}", id)
        }
    }
}

pub fn clear_sockaddr(addr: *mut u8, addrlen: *mut socklen_t) {
    if addrlen.is_null() {
        return;
    }

    unsafe {
        let requested_len = *addrlen as usize;
        if !addr.is_null() && requested_len > 0 {
            ptr::write_bytes(
                addr,
                0,
                requested_len.min(mem::size_of::<sockaddr_storage>()),
            );
        }
        *addrlen = 0;
    }
}
