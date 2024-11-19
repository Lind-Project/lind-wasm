#![allow(dead_code)]
use crate::interface;
//going to get the datatypes and errnos from the cage file from now on
pub use crate::interface::errnos::{syscall_error, Errno};
pub use crate::interface::types::{
    Arg, EpollEvent, FSData, IoctlPtrUnion, PipeArray, PollStruct, Rlimit, ShmidsStruct, StatData,
};

use super::filesystem::normpath;
use super::net::SocketHandle;
pub use super::syscalls::fs_constants::*;
pub use super::syscalls::net_constants::*;
pub use super::syscalls::sys_constants::*;

pub use crate::interface::CAGE_TABLE;

#[derive(Debug, Clone)]
pub enum FileDescriptor {
    File(FileDesc),
    Stream(StreamDesc),
    Socket(SocketDesc),
    Pipe(PipeDesc),
    Epoll(EpollDesc),
}

#[derive(Debug, Clone)]
pub struct FileDesc {
    pub position: usize,
    pub inode: usize,
    pub flags: i32,
    pub advlock: interface::RustRfc<interface::AdvisoryLock>,
}

#[derive(Debug, Clone)]
pub struct StreamDesc {
    pub position: usize,
    pub stream: i32, //0 for stdin, 1 for stdout, 2 for stderr
    pub flags: i32,
    pub advlock: interface::RustRfc<interface::AdvisoryLock>,
}

#[derive(Debug, Clone)]
pub struct SocketDesc {
    pub flags: i32,
    pub domain: i32,
    pub rawfd: i32,
    pub handle: interface::RustRfc<interface::RustLock<SocketHandle>>,
    pub advlock: interface::RustRfc<interface::AdvisoryLock>,
}

#[derive(Debug, Clone)]
pub struct PipeDesc {
    pub pipe: interface::RustRfc<interface::EmulatedPipe>,
    pub flags: i32,
    pub advlock: interface::RustRfc<interface::AdvisoryLock>,
}

#[derive(Debug, Clone)]
pub struct EpollDesc {
    pub mode: i32,
    pub registered_fds: interface::RustHashMap<i32, EpollEvent>,
    pub advlock: interface::RustRfc<interface::AdvisoryLock>,
    pub errno: i32,
    pub flags: i32,
}

pub type FdTable = Vec<interface::RustRfc<interface::RustLock<Option<FileDescriptor>>>>;

#[derive(Debug)]
pub struct Cage {
    pub cageid: u64,
    pub cwd: interface::RustLock<interface::RustRfc<interface::RustPathBuf>>,
    pub parent: u64,
    pub filedescriptortable: FdTable,
    pub cancelstatus: interface::RustAtomicBool,
    pub getgid: interface::RustAtomicI32,
    pub getuid: interface::RustAtomicI32,
    pub getegid: interface::RustAtomicI32,
    pub geteuid: interface::RustAtomicI32,
    pub rev_shm: interface::Mutex<Vec<(u32, i32)>>, //maps addr within cage to shmid
    pub mutex_table: interface::RustLock<Vec<Option<interface::RustRfc<interface::RawMutex>>>>,
    pub cv_table: interface::RustLock<Vec<Option<interface::RustRfc<interface::RawCondvar>>>>,
    pub sem_table: interface::RustHashMap<u32, interface::RustRfc<interface::RustSemaphore>>,
    pub thread_table: interface::RustHashMap<u64, bool>,
    pub signalhandler: interface::RustHashMap<i32, interface::SigactionStruct>,
    pub sigset: interface::RustHashMap<u64, interface::RustAtomicU64>,
    pub pendingsigset: interface::RustHashMap<u64, interface::RustAtomicU64>,
    pub main_threadid: interface::RustAtomicU64,
    pub interval_timer: interface::IntervalTimer,
}

impl Cage {
    pub fn get_next_fd(
        &self,
        startfd: Option<i32>,
    ) -> (
        i32,
        Option<interface::RustLockWriteGuard<Option<FileDescriptor>>>,
    ) {
        let start = match startfd {
            Some(startfd) => startfd,
            None => STARTINGFD,
        };

        // let's get the next available fd number. The standard says we need to return the lowest open fd number.
        for fd in start..MAXFD {
            let fdguard = self.filedescriptortable[fd as usize].try_write();
            if let Some(ref fdopt) = fdguard {
                // we grab the lock here and if there is no occupied cage, we return the fdno and guard while keeping the fd slot locked
                if fdopt.is_none() {
                    return (fd, fdguard);
                }
            }
        }
        return (
            syscall_error(
                Errno::ENFILE,
                "get_next_fd",
                "no available file descriptor number could be found",
            ),
            None,
        );
    }

    pub fn changedir(&self, newdir: interface::RustPathBuf) {
        let newwd = interface::RustRfc::new(normpath(newdir, self));
        let mut cwdbox = self.cwd.write();
        *cwdbox = newwd;
    }

    // function to signal all cvs in a cage when forcing exit
    pub fn signalcvs(&self) {
        let cvtable = self.cv_table.read();

        for cv_handle in 0..cvtable.len() {
            if cvtable[cv_handle as usize].is_some() {
                let clonedcv = cvtable[cv_handle as usize].as_ref().unwrap().clone();
                clonedcv.broadcast();
            }
        }
    }

    pub fn send_pending_signals(&self, sigset: interface::SigsetType, pthreadid: u64) {
        for signo in 1..SIGNAL_MAX {
            if interface::lind_sigismember(sigset, signo) {
                interface::lind_threadkill(pthreadid, signo);
            }
        }
    }

    pub fn get_filedescriptor(
        &self,
        fd: i32,
    ) -> Result<interface::RustRfc<interface::RustLock<Option<FileDescriptor>>>, ()> {
        if (fd < 0) || (fd >= MAXFD) {
            Err(())
        } else {
            Ok(self.filedescriptortable[fd as usize].clone())
        }
    }
}

pub fn init_fdtable() -> FdTable {
    let mut fdtable = Vec::new();
    // load lower handle stubs
    let stdin = interface::RustRfc::new(interface::RustLock::new(Some(FileDescriptor::Stream(
        StreamDesc {
            position: 0,
            stream: 0,
            flags: O_RDONLY,
            advlock: interface::RustRfc::new(interface::AdvisoryLock::new()),
        },
    ))));
    let stdout = interface::RustRfc::new(interface::RustLock::new(Some(FileDescriptor::Stream(
        StreamDesc {
            position: 0,
            stream: 1,
            flags: O_WRONLY,
            advlock: interface::RustRfc::new(interface::AdvisoryLock::new()),
        },
    ))));
    let stderr = interface::RustRfc::new(interface::RustLock::new(Some(FileDescriptor::Stream(
        StreamDesc {
            position: 0,
            stream: 2,
            flags: O_WRONLY,
            advlock: interface::RustRfc::new(interface::AdvisoryLock::new()),
        },
    ))));
    fdtable.push(stdin);
    fdtable.push(stdout);
    fdtable.push(stderr);

    for _fd in 3..MAXFD as usize {
        fdtable.push(interface::RustRfc::new(interface::RustLock::new(None)));
    }
    fdtable
}

pub fn create_unix_sockpipes() -> (
    interface::RustRfc<interface::EmulatedPipe>,
    interface::RustRfc<interface::EmulatedPipe>,
) {
    let pipe1 = interface::RustRfc::new(interface::new_pipe(UDSOCK_CAPACITY));
    let pipe2 = interface::RustRfc::new(interface::new_pipe(UDSOCK_CAPACITY));

    (pipe1, pipe2)
}
