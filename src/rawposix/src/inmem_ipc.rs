use fdtables::FDTableEntry;
use libc::{c_void, sockaddr, sockaddr_in, sockaddr_un, socklen_t};
use once_cell::sync::Lazy;
use std::collections::{HashMap, VecDeque};
use std::ffi::CStr;
use std::mem;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, Weak};
use sysdefs::constants::err_const::{syscall_error, Errno};
use sysdefs::constants::fs_const::O_NONBLOCK;
use sysdefs::constants::lind_platform_const::{FDKIND_IMPIPE, FDKIND_IMSOCK};

const PIPE_CAPACITY: usize = 65_536;
const EPHEMERAL_PORT_START: u16 = 49_152;

static NEXT_ENDPOINT_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_SOCKET_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_EPHEMERAL_PORT: AtomicU64 = AtomicU64::new(EPHEMERAL_PORT_START as u64);

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
    buf: Mutex<VecDeque<u8>>,
    readable: Condvar,
    writable: Condvar,
    capacity: usize,
    readers_open: AtomicBool,
    writers_open: AtomicBool,
}

impl ByteQueue {
    fn new(capacity: usize) -> Self {
        Self {
            buf: Mutex::new(VecDeque::with_capacity(capacity)),
            readable: Condvar::new(),
            writable: Condvar::new(),
            capacity,
            readers_open: AtomicBool::new(true),
            writers_open: AtomicBool::new(true),
        }
    }

    fn read(&self, dst: *mut u8, count: usize, nonblocking: bool) -> i32 {
        if count == 0 {
            return 0;
        }

        let mut guard = self.buf.lock().unwrap();
        loop {
            if !guard.is_empty() {
                let n = count.min(guard.len());
                for i in 0..n {
                    unsafe {
                        *dst.add(i) = guard.pop_front().unwrap();
                    }
                }
                self.writable.notify_all();
                return n as i32;
            }

            if !self.writers_open.load(Ordering::Acquire) {
                return 0;
            }

            if nonblocking {
                return syscall_error(Errno::EAGAIN, "inmem_read", "would block");
            }

            guard = self.readable.wait(guard).unwrap();
        }
    }

    fn write(&self, src: *const u8, count: usize, nonblocking: bool) -> i32 {
        if count == 0 {
            return 0;
        }

        let src_slice = unsafe { std::slice::from_raw_parts(src, count) };
        let mut written = 0usize;

        while written < count {
            let mut guard = self.buf.lock().unwrap();
            while guard.len() == self.capacity {
                if !self.readers_open.load(Ordering::Acquire) {
                    return syscall_error(Errno::EPIPE, "inmem_write", "reader closed");
                }
                if written > 0 {
                    return written as i32;
                }
                if nonblocking {
                    return syscall_error(Errno::EAGAIN, "inmem_write", "would block");
                }
                guard = self.writable.wait(guard).unwrap();
            }

            if !self.readers_open.load(Ordering::Acquire) {
                return syscall_error(Errno::EPIPE, "inmem_write", "reader closed");
            }

            let available = self.capacity - guard.len();
            let n = (count - written).min(available);
            guard.extend(&src_slice[written..written + n]);
            written += n;
            self.readable.notify_all();
        }

        written as i32
    }

    fn close_readers(&self) {
        self.readers_open.store(false, Ordering::Release);
        self.writable.notify_all();
    }

    fn close_writers(&self) {
        self.writers_open.store(false, Ordering::Release);
        self.readable.notify_all();
    }

    fn poll_read(&self) -> i16 {
        let has_data = !self.buf.lock().unwrap().is_empty();
        let writers_open = self.writers_open.load(Ordering::Acquire);
        if has_data || !writers_open {
            libc::POLLIN as i16
        } else {
            0
        }
    }

    fn poll_write(&self) -> i16 {
        if !self.readers_open.load(Ordering::Acquire) {
            return libc::POLLERR as i16;
        }
        if self.buf.lock().unwrap().len() < self.capacity {
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
    pending: VecDeque<Arc<InmemSocket>>,
}

impl InmemSocket {
    fn new(domain: i32, socktype: i32, protocol: i32) -> Self {
        Self {
            incoming: Arc::new(ByteQueue::new(PIPE_CAPACITY)),
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
                pending: VecDeque::new(),
            }),
            accept_ready: Condvar::new(),
        }
    }
}

pub fn enabled() -> bool {
    std::env::var("LIND_RAWPOSIX_INMEM_IPC")
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
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

pub fn read(fdentry: FDTableEntry, dst: *mut u8, count: usize) -> i32 {
    match fdentry.fdkind {
        FDKIND_IMPIPE => match get_endpoint(fdentry.underfd) {
            Ok(endpoint) if endpoint.side == PipeSide::Read => {
                endpoint.queue.read(dst, count, fd_is_nonblocking(fdentry))
            }
            Ok(_) => syscall_error(Errno::EBADF, "read", "pipe is not readable"),
            Err(e) => e,
        },
        FDKIND_IMSOCK => match get_socket(fdentry.underfd) {
            Ok(socket) => {
                if socket.state.lock().unwrap().read_shutdown {
                    return 0;
                }
                socket.incoming.read(dst, count, fd_is_nonblocking(fdentry))
            }
            Err(e) => e,
        },
        _ => syscall_error(Errno::EBADF, "read", "unsupported in-memory fd"),
    }
}

pub fn write(fdentry: FDTableEntry, src: *const u8, count: usize) -> i32 {
    match fdentry.fdkind {
        FDKIND_IMPIPE => match get_endpoint(fdentry.underfd) {
            Ok(endpoint) if endpoint.side == PipeSide::Write => {
                endpoint.queue.write(src, count, fd_is_nonblocking(fdentry))
            }
            Ok(_) => syscall_error(Errno::EBADF, "write", "pipe is not writable"),
            Err(e) => e,
        },
        FDKIND_IMSOCK => match get_socket(fdentry.underfd) {
            Ok(socket) => {
                let peer = {
                    let state = socket.state.lock().unwrap();
                    if state.write_shutdown {
                        return syscall_error(Errno::EPIPE, "write", "socket write shut down");
                    }
                    state.peer.as_ref().and_then(Weak::upgrade)
                };
                match peer {
                    Some(peer) => peer.incoming.write(src, count, fd_is_nonblocking(fdentry)),
                    None => syscall_error(Errno::EPIPE, "write", "socket peer closed"),
                }
            }
            Err(e) => e,
        },
        _ => syscall_error(Errno::EBADF, "write", "unsupported in-memory fd"),
    }
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
    if listeners.contains_key(&key) {
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

pub fn is_loopback_sockaddr(addr: *mut u8, addrlen: socklen_t) -> bool {
    matches!(sockaddr_key(addr, addrlen), Some((family, _)) if family == libc::AF_INET)
}

pub fn sendto(fdentry: FDTableEntry, buf: *const c_void, buflen: usize) -> i32 {
    write(fdentry, buf as *const u8, buflen)
}

pub fn recvfrom(fdentry: FDTableEntry, buf: *mut c_void, buflen: usize) -> i32 {
    read(fdentry, buf as *mut u8, buflen)
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
    if (ip >> 24) != 127 {
        return None;
    }

    let port = u16::from_be(sockaddr.sin_port);
    Some(format!("inet:{}", port))
}

fn next_ephemeral_key(domain: i32) -> String {
    match domain {
        libc::AF_INET => {
            let port = NEXT_EPHEMERAL_PORT.fetch_add(1, Ordering::Relaxed) as u16;
            format!("inet:{}", port)
        }
        _ => {
            let id = NEXT_SOCKET_ID.load(Ordering::Relaxed);
            format!("unix:@autobind{}", id)
        }
    }
}

pub fn clear_sockaddr(addr: *mut u8, addrlen: *mut socklen_t) {
    if !addr.is_null() {
        unsafe {
            ptr::write_bytes(addr, 0, mem::size_of::<sockaddr_un>());
        }
    }
    if !addrlen.is_null() {
        unsafe {
            *addrlen = 0;
        }
    }
}
