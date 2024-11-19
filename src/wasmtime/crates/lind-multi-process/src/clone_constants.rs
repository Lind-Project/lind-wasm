#[derive(Copy, Clone, Default, Debug)]
#[repr(C)]
pub struct CloneArgStruct {
    pub flags: u64,           // Flags that control the behavior of the child process
    pub pidfd: u64,           // File descriptor to receive the child's PID
    pub child_tid: u64,       // Pointer to a memory location where the child TID will be stored
    pub parent_tid: u64,      // Pointer to a memory location where the parent's TID will be stored
    pub exit_signal: u64,     // Signal to be sent when the child process exits
    pub stack: u64,           // Address of the stack for the child process
    pub stack_size: u64,      // Size of the stack for the child process
    pub tls: u64,             // Thread-Local Storage (TLS) descriptor for the child thread
    pub set_tid: u64,         // Pointer to an array of TIDs to be set in the child
    pub set_tid_size: u64,    // Number of TIDs in the `set_tid` array
    pub cgroup: u64,          // File descriptor for the cgroup to which the child process should be attached
}

/* Cloning flags.  */
pub const CSIGNAL: u64 =       0x000000ff; /* Signal mask to be sent at exit.  */
pub const CLONE_VM: u64 =      0x00000100; /* Set if VM shared between processes.  */
pub const CLONE_FS: u64 =      0x00000200; /* Set if fs info shared between processes.  */
pub const CLONE_FILES: u64 =   0x00000400; /* Set if open files shared between processes.  */
pub const CLONE_SIGHAND: u64 = 0x00000800; /* Set if signal handlers shared.  */
pub const CLONE_PIDFD: u64 =   0x00001000; /* Set if a pidfd should be placed in parent.  */
pub const CLONE_PTRACE: u64 =  0x00002000; /* Set if tracing continues on the child.  */
pub const CLONE_VFORK: u64 =   0x00004000; /* Set if the parent wants the child to wake it up on mm_release.  */
pub const CLONE_PARENT: u64 =  0x00008000; /* Set if we want to have the same parent as the cloner.  */
pub const CLONE_THREAD: u64 =  0x00010000; /* Set to add to same thread group.  */
pub const CLONE_NEWNS: u64 =   0x00020000; /* Set to create new namespace.  */
pub const CLONE_SYSVSEM: u64 = 0x00040000; /* Set to shared SVID SEM_UNDO semantics.  */
pub const CLONE_SETTLS: u64 =  0x00080000; /* Set TLS info.  */
pub const CLONE_PARENT_SETTID: u64 = 0x00100000; /* Store TID in userlevel buffer before MM copy.  */
pub const CLONE_CHILD_CLEARTID: u64 = 0x00200000; /* Register exit futex and memory location to clear.  */
pub const CLONE_DETACHED: u64 = 0x00400000; /* Create clone detached.  */
pub const CLONE_UNTRACED: u64 = 0x00800000; /* Set if the tracing process can't force CLONE_PTRACE on this clone.  */
pub const CLONE_CHILD_SETTID: u64 = 0x01000000; /* Store TID in userlevel buffer in the child.  */
pub const CLONE_NEWCGROUP: u64 =    0x02000000;	/* New cgroup namespace.  */
pub const CLONE_NEWUTS: u64 =	0x04000000;	/* New utsname group.  */
pub const CLONE_NEWIPC: u64 =	0x08000000;	/* New ipcs.  */
pub const CLONE_NEWUSER: u64 =	0x10000000;	/* New user namespace.  */
pub const CLONE_NEWPID: u64 =	0x20000000;	/* New pid namespace.  */
pub const CLONE_NEWNET: u64 =	0x40000000;	/* New network namespace.  */
pub const CLONE_IO: u64 =	0x80000000;	/* Clone I/O context.  */
/* cloning flags intersect with CSIGNAL so can be used only with unshare and
   clone3 syscalls.  */
pub const CLONE_NEWTIME: u64 =	0x00000080;      /* New time namespace */
