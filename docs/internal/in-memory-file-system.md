## In Memory File System 

### Overview 

The In Memory File System (IMFS) provides a self-contained implementation of a POSIX-like FS backed by memory. It serves as a backbone that can later be integrated as a grate to sandbox any FS calls made by a cage. IMFS exposts POSIX-like APIs and maintains its own inode and file descriptor tables to provide an end-to-end FS interface.

IMFS is independent of the Lind and can run natively on Linux, with the limitation that it only supports a single process. New features are usually developed and tested natively before being integrated and tested within Lind. 

### File System APIs

IMFS mirrors POSIX system calls with the the added `cageid` paramter. For example:

```
open(const char* pathname, int flags, mode_t mode)
->
imfs_open(int cageid, const char* pathname, int flags, mode_t mode)
```

The behaviour of these APIs closely matches those of Linux's system calls, including types, return values, and error codes. This ensure that IMFS can be a drop-in replacement for an actual filesystem. 

### Utility Functions. 

In addition to POSIX APIs, IMFS also provides helper functions for moving files in and out of memory. 

```
load_file(char *path); // Load a single file into IMFS at 'path', recursively creating any required folders. 

dump_file(char *path, char *actual_path); // Copies IMFS file at 'path' to the host filesystem at 'actual_path'

preloads(char *preload_files); // 'preload_files' is expected to be a ':' separated list of filenames that are each passed to load_file()
```

Typically, `preloads` or `load_file` is called at the start of a grate's lifecycle to stage the required files, and `dump_file` is called at the end to persist any results back to the host. 

### Implementation

IMFS tracks files and directories through an array of `struct Node` which is equivalent to an inode (file, directory, symlink, or pipe). Node allocations are implemented through the use of a free list and a pointer that maintains the next free slot in the array. The data stored in a Node is specific to the node's type. A directory stores information about child nodes, and a symlink stores a pointer to the linked node. For a regular file, data is stored in `Chunks` that each contain 1024 bytes of data. These chunks are linked through a linked list, and the node stores the pointer to the head and the tail of this list. 

Each cage has its own array of `struct FileDesc` objects that represent a file descriptor. The file descriptors returned and used by the FS functions are indices into this array. FD allocations start at descriptor 3, the implementation for standard descriptors (stdin, stdout, stderr) are left to the enclosing grate. FileDescs are allocated through `open` or `openat` calls. Upon allocation, the FileDesc stores a pointer to the Node that was opened, the read offset, and the open flags. 

### Testing 

Testing for POSIX-compliance is checked through using `pjdfstest` which is a standard test suite used for BSD and Linux file systems. The tests are run natively on Linux which required modifying `pjdfstest` to have a persistent test runner to maintain state. `pjdfstest` provides a comprehensive list of one-liner assertions that are helpful to discover edge-cases and violations of the POSIX standards. 

### Future Work

- Currently only a handful of the most common logical branches are supported for most syscalls. For example, not all flags are supported for `open`. 
- Access control is not implemented, by default all nodes are created with mode `0777` allowing for any user or group to access them. 
- `mmap` is yet to be implemented. 
- Performance testing for reading and writing. 
