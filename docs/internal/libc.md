# Modifications Made to glibc and `crt1.c`

## 1. Changes to glibc

### 1.1 Changing the System Call Mechanism
The system call mechanism was modified to route system calls through `rawposix` instead of directly invoking the kernel. The new format for making system calls is structured as follows:

```c
MAKE_SYSCALL(syscallnum, "syscall|callname", arg1, arg2, arg3, arg4, arg5, arg6)
```

For each system call file in **glibc**, a header file named `syscall-template.h` was added with the following content:

```c
#include <sys/syscall.h>
#include <stdint.h>    // For uint64_t
#include <unistd.h>
#include <lind_syscall.h>

// Define NOTUSED for unused arguments
#define NOTUSED 0xdeadbeefdeadbeefULL

// Macro to create a syscall and redirect it to rawposix
#define MAKE_SYSCALL(syscallnum, callname, arg1, arg2, arg3, arg4, arg5, arg6) \
    lind_syscall(syscallnum, \
                 (unsigned long long)(callname), \
                 (unsigned long long)(arg1), \
                 (unsigned long long)(arg2), \
                 (unsigned long long)(arg3), \
                 (unsigned long long)(arg4), \
                 (unsigned long long)(arg5), \
                 (unsigned long long)(arg6))
```

The `MAKE_SYSCALL` macro redirects system calls to **rawposix**, providing an interface for syscall handling in this context.

### 1.2 Eliminating Assembly Code
Since WebAssembly (WASM) does not support assembly, all assembly-related components in glibc were removed:
- Inline assembly code was rewritten in C.
- Files ending in `.s` were converted to `.c` files, and their functionalities were reimplemented in C.

### 1.3 Handling Automatically Generated `.s` Files
glibc automatically generates `.s` files for certain system calls. To address this:
- The script responsible for generating these `.s` files was disabled.
- The corresponding system calls were manually implemented in C and placed in appropriate `.c` files.

### 1.4 Additional Modifications
- **Disable `_dl_mcount_wrapper_check`**: This functionality was disabled as it is not required in the WASI environment.
- **Change `initial-exec` to `local-exec`**: All instances of `initial-exec` were replaced with `local-exec` to align with WebAssembly's threading and memory model.
- **Implement `BYTE_COPY_FWD` and `BYTE_COPY_BWD`**: These functions were implemented in C without relying on `memcpy` or `memmove` to ensure compatibility with the WASM environment.
- **Disable `attribute_relro`**: The original C code places the vtable into the `relro` section in the binary. Since WebAssembly binaries do not have this section, the attribute was disabled.

### 1.5 Disabling Auto-Generated Assembly in `i386` Sysdeps

glibc’s `sysdeps/unix/sysv/linux/i386` hierarchy, including the `i686` subdirectory, relies on build-time logic that automatically generates assembly (`.S`) files for syscall stubs and low-level ABI glue. This mechanism assumes a native assembler and is incompatible with WebAssembly.

To support WASM, the assembly auto-generation logic in these directories was disabled. All generated `.S` files were replaced with manually provided C implementations that preserve the required behavior without relying on architecture-specific assembly. This prevents glibc from emitting or compiling assembly code when targeting WASM while keeping the changes localized to the `i386` Linux sysdeps.


## 2. Modifications and Additions to `crt1.c` for WASI

### 2.1 WASI-Specific Function Wrappers
- **Wrappers for WASI Snapshot Preview 1 APIs**:
  Several wrappers were defined for handling WASI `args` and `environ` APIs, including:
  - `__imported_wasi_snapshot_preview1_args_sizes_get`
  - `__imported_wasi_snapshot_preview1_args_get`
  - `__imported_wasi_snapshot_preview1_environ_get`
  - `__imported_wasi_snapshot_preview1_environ_sizes_get`

- **Implementation**:
  These wrappers use the following attribute for integration:
  ```c
  __attribute__((
      __import_module__("wasi_snapshot_preview1"),
      __import_name__("function_name")
  ));
  ```

- **Purpose**: Enables access to WASI-specific argument and environment APIs.

### 2.2 Environment Initialization
- **Added `__wasi_initialize_environ`**:
  This function initializes the environment variables by:
  - Using `__wasi_environ_sizes_get` to determine the size of environment data.
  - Using `__wasi_environ_get` to populate the `environ` array.

- **Fallback Logic**:
  If the environment is empty, the program:
  - Falls back to a static empty environment (`empty_environ`).
  - Exits with an appropriate error code when necessary.

### 2.3 Thread and TLS Setup
- **Added Calls to `__libc_setup_tls` and `__wasi_init_tp`**:
  These functions are included in `_start` to set up thread-local storage (TLS) and thread pointers, which are essential for multithreading or TLS-dependent code.

### 2.4 Main Function Handling
- **Modified `__main_void` to Handle WASI Arguments**:
  - Initializes command-line arguments by:
    - Using `__wasi_args_sizes_get` to determine argument buffer sizes.
    - Using `__wasi_args_get` to populate `argv` and `argv_buf`.
  - Passes the initialized arguments to `__main_argc_argv`.

- **Weak Symbol for `__main_argc_argv`**:
  Defined as a weak symbol to allow the dynamic linker to handle cases where no `main` function exists (e.g., in reactor-style applications).

### 2.5 Error Handling
- **Specific Exit Codes**:
  - Introduced `_Exit(EX_OSERR)` and `_Exit(EX_SOFTWARE)` for different error scenarios, aligning with `sysexits.h` standards.

- **Purpose**: Provides descriptive and standard error handling for memory allocation or initialization failures.

### 2.6 Placeholder Functions
- **Added Stub for `__wasm_call_dtors`**:
  - An empty placeholder function for future destructor handling.

- **Added Stub for `__wasi_proc_exit`**:
  - A placeholder function for handling process exits in WASI.

### 2.7 Memory Allocation for `argv` and `environ`
- Allocates memory dynamically for:
  - Argument buffers (`argv_buf`) and pointers (`argv`).
  - Environment buffers (`environ_buf`) and pointers (`environ_ptrs`).
- Uses `malloc` and `calloc` with robust error handling to prevent memory allocation failures.