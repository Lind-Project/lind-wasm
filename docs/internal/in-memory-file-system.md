# In Memory File System 
 
## Overview 

The In Memory File System (IMFS) provides a self-contained implementation of a POSIX-like FS backed by memory. It serves as a backbone that can later be integrated as a grate to sandbox any FS calls made by a cage. IMFS exposts POSIX-like APIs and maintains its own inode and file descriptor tables to provide an end-to-end FS interface.

IMFS is independent of Lind and can run natively on Linux, with the limitation that it only supports a single process. New features are usually developed and tested natively before being integrated and tested within Lind. 

## File System APIs

IMFS mirrors POSIX system calls with an added `cageid` parameter. For example:

```
open(const char* pathname, int flags, mode_t mode)
->
imfs_open(int cageid, const char* pathname, int flags, mode_t mode)
```

The behaviours of these APIs closely match those of their corresponding Linux system calls. They follow the semantics described in man pages including types, return valies, and error codes. This allows IMFS to be a drop-in replacement for a conventional filesystem. 

As mentioned earlier, it is possible to run IMFS natively, and it requires the `cageid` parameter to be stubbed as an integer constant between `[0, 128)` 

```
#define CAGEID 0

int fd = imfs_open(CAGEID, "/testfile.txt", O_RDONLY, 0); 
imfs_close(CAGEID, fd);
```
## Utility Functions. 

In addition to POSIX APIs, IMFS also provides helper functions for moving files in and out of memory. 

- `load_file(char *path)` Load a single file into IMFS at `path`, recursively creating any required folders. 

- `dump_file(char *path, char *actual_path)` Copy IMFS file at `path` to the host filesystem at `actual_path`

- `preloads(char *preload_files)` Copy files from host to IMFS, `preload_files` being a `:` separated list of filenames. 

These utility functions are typically called at the beginning and the end of a grate's lifecycle. `load_file` and `preloads` are used to stage files into memory, and `dump_file` is used to persist results back to the host system.

## Implementation

### Inodes 

IMFS maintains an array of `Node` objects each of which serve as an inode to represent an FS object (file, directory, symlink, or pipe). Allocation of nodes is performed using a free-list mechanism along with a pointer that tracks the next available slot within the array. 

The structure of the node is specialized according to its type:

- Directories contain references to child nodes.
- Symlinks maintain a pointer to the target node. 
- Regular files store data in fixed-sized `Chunk`s, each of which store 1024 bytes of data. These chunks are organized as a singly linked list. 

### File Descriptors

Each cage is associated with its own array of `FileDesc` objects that represent a file descriptor. The file descriptors used by these FS calls return indices into this array. 

File descriptor allocation begins at index 3. The management of standard descriptors (`stdin`, `stdout`, `stderr`) are delegated to the enclosing grate.

Descriptors are allocated using `imfs_open` or `imfs_openat`. Each file descriptor object stores:

- A pointer to the associated node. 
- The current file offset. 
- Open flags

## Building

### Native Build

- `make lib` to build as a library
- `make imfs` to build with the main function
- `make debug` build with debug symbols

### Lind Integration Build

The following compile flags are required to compile IMFS for a Lind build:

- `-DLIB` omit the main function
- `-DDIAG` to enable diagnostic logging
- `-D_GNU_SOURCE` needed to support `SEEK_HOLE` and `SEEK_DATA` operations in `imfs_lseek()`

## Testing 

POSIX compliance is validate through `pjdfstest`, a widely adopted test suite for file systems for both BSD and Linux file systems. The tests are executed natively on Linux, which required modifications to `pjdfstest` in order to support a persisten test runner capabla of maintaining FS state. 

`pdjfstest` provides a comprehensive list of assertions each designed to verify a specific FS property. This approach allows for easier detection of edge-cases. 

The test suite is invoked using:

- `make test` run all tests
- `make test-<feature>` run all tests in a particular feature

## Future Work

- Currently only a handful of the most common logical branches are supported for most syscalls. For example, not all flags are supported for `open`. 
- Access control is not implemented, by default all nodes are created with mode `0777` allowing for any user or group to access them. 
- `mmap` is yet to be implemented. 
- Performance testing for reading and writing. 
- Integrating FD table management with `fdtables` crate.

