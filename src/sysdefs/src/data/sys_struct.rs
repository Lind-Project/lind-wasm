#[derive(Copy, Clone, Default, Debug)]
#[repr(C)]
pub struct CloneArgStruct {
    pub flags: u64,        // Flags that control the behavior of the child process
    pub pidfd: u64,        // File descriptor to receive the child's PID
    pub child_tid: u64,    // Pointer to a memory location where the child TID will be stored
    pub parent_tid: u64,   // Pointer to a memory location where the parent's TID will be stored
    pub exit_signal: u64,  // Signal to be sent when the child process exits
    pub stack: u64,        // Address of the stack for the child process
    pub stack_size: u64,   // Size of the stack for the child process
    pub tls: u64,          // Thread-Local Storage (TLS) descriptor for the child thread
    pub set_tid: u64,      // Pointer to an array of TIDs to be set in the child
    pub set_tid_size: u64, // Number of TIDs in the `set_tid` array
    pub cgroup: u64, // File descriptor for the cgroup to which the child process should be attached
}
