# RawPOSIX

RawPOSIX is the [microvisor](../index.md#microvisor-rawposix) of Lind-Wasm: a small, trusted Rust component that implements system calls on behalf of [cages](../index.md#cages). It is a library crate at `src/rawposix`, linked into the `lind-boot` host process. When a cage issues a system call and no [grate](grates.md) intercepts it, the call is dispatched by [3i](3i.md) to a RawPOSIX handler, which validates and converts the arguments and then performs the operation, in most cases by issuing a real Linux system call through libc.

The "raw" in the name describes the implementation strategy. Rather than reimplementing OS subsystems, RawPOSIX forwards work to the host kernel wherever possible and keeps only the per-cage state needed to give each cage its own coherent view: virtual file descriptor tables, memory maps, working directory, parent and child relationships, and signal state. This is what lets many isolated cages run inside a single Linux process while each behaves like a separate POSIX process.

On compatibility: RawPOSIX aims to preserve POSIX behavior for the system calls it implements. It is not a complete POSIX implementation, and no strict compliance guarantee is intended. The authoritative list of supported calls is the dispatch table in `src/rawposix/src/syscall_table.rs`, and behavior can differ from native Linux where the WebAssembly runtime or the cage model requires it (for example, the 32-bit guest address space, virtual file descriptors, and cage IDs in place of kernel PIDs).

## What RawPOSIX is responsible for

- **Implementing supported system calls.** Each entry in `syscall_table.rs` maps a Linux syscall number to a handler function in one of three modules: `fs_calls.rs` (files, directories, `mmap`/`brk`, shared memory), `net_calls.rs` (sockets, `poll`/`select`/`epoll`), and `sys_calls.rs` (`fork`, `exec`, `exit`, `waitpid`, IDs, signal-related calls).
- **Per-cage resource isolation.** Translating each cage's virtual file descriptors to kernel file descriptors (via the `fdtables` crate), maintaining per-cage memory maps (`vmmap`), and tracking process relationships between cages.
- **Registering itself as the default syscall handler.** At startup, `register_rawposix_syscall` in `init.rs` walks the syscall table and registers every handler with 3i for the initial cage, so that uninterposed syscalls reach RawPOSIX.
- **Runtime lifecycle.** `rawposix_start` bootstraps the environment (cage table, file descriptor tables, the init cage with cage ID 1, standard I/O descriptors), and `rawposix_shutdown` exits any remaining cages.

## What it is not responsible for

- **Executing WebAssembly.** Wasmtime (driven by `lind-boot`) runs cage code and owns the linear memory.
- **Syscall routing and interposition policy.** [3i](3i.md) owns the per-cage handler tables; [grates](grates.md) implement interception logic. RawPOSIX is simply the handler that uninterposed calls land on.
- **The C library that applications see.** [lind-glibc](libc.md) implements the userspace side and decides how a libc call becomes a Lind syscall.
- **Filesystem namespace isolation.** `lind-boot` chroots into the `lindfs` directory before starting RawPOSIX, so path-based syscalls are already confined by the time RawPOSIX forwards them to the kernel.
- **Scheduling.** Cage threads are ordinary host threads scheduled by the Linux kernel; preemption for signals and cage termination is handled by Wasmtime's epoch mechanism.

## Life of a syscall

The path of a `read(fd, buf, count)` call from a cage down to the host kernel:

```
cage (WebAssembly)                      host process (native)

read() in lind-glibc
 └─ MAKE_SYSCALL macro
     └─ make_threei_call
         └─ import lind::make-syscall ──► host func (lind-common, Wasmtime)
                                           └─ 3i make_syscall
                                               └─ per-cage handler table lookup
                                                   └─ read_syscall (RawPOSIX fs_calls.rs)
                                                       └─ libc::read ──► Linux kernel
```

1. **lind-glibc.** The guest's `read()` reaches a `MAKE_SYSCALL` site (see `src/glibc/sysdeps/unix/syscall-template.h`), which calls `make_threei_call` in `src/glibc/lind_syscall/lind_syscall.c`. Pointer arguments are translated from guest linear-memory offsets to host virtual addresses here (`TRANSLATE_ARG_TO_HOST` in `addr_translation.h`).
2. **Crossing the sandbox boundary.** `make_threei_call` invokes `__lind_make_syscall_trampoline`, which is a WebAssembly import with module `lind` and name `make-syscall`.
3. **Wasmtime.** The runtime supplies that import: `add_syscall_to_linker` in `src/wasmtime/crates/lind-common/src/lib.rs` registers a host function for `lind::make-syscall`. After handling Asyncify replay cases (relevant for `fork`/`exec`/`exit`), it forwards the call to 3i's `make_syscall`.
4. **3i dispatch.** 3i looks up the handler registered for this cage and syscall number in the cage's handler table. For a cage with no interposition, that handler is the RawPOSIX implementation registered at startup. A grate may be registered instead, in which case the grate decides whether and how to forward the call.
5. **RawPOSIX handler.** `read_syscall` in `src/rawposix/src/fs_calls.rs` converts the virtual fd to a kernel fd (`convert_fd_to_host`), converts the buffer pointer and count (`sc_convert_buf`, `sc_convert_sysarg_to_usize` from the `typemap` crate), verifies that unused argument slots are actually unused, and calls `libc::read`.
6. **Return path.** Handlers return an `i32`, with errors encoded as `-errno`. Back in glibc, `make_threei_call` translates a negative return into `errno` plus a `-1` return value, following the usual POSIX convention. A few call sites (such as futex operations) opt out of this translation and consume the raw value.

## Per-cage state

The `Cage` struct lives in the sibling crate `src/cage`, and RawPOSIX manipulates it through a global cage table keyed by cage ID (`cagetable_getref`). A cage holds, among other fields, its working directory, parent cage ID, `vmmap` (the per-cage memory map, tracked in page units; see [Memory](memory.md)), signal handler and pending-signal state, and zombie children for `waitpid`. Virtual file descriptors are kept outside the `Cage` struct, in per-cage tables owned by the `fdtables` crate; every fd-taking syscall translates its virtual fd before touching the kernel.

Process-like semantics are built from this state: `fork` creates a new cage that inherits copies of the parent's fd table and memory mappings, `exec` replaces a cage's contents, and `exit`/`waitpid` update the parent's zombie list. Details are covered in [Multi-Processing](multiprocess-support.md).

### The handler ABI

Every RawPOSIX handler has the same C-ABI signature, `RawCallFunc` in `src/rawposix/src/init.rs`: a `target_cageid` followed by six argument pairs, where each pair is a raw `u64` value and the ID of the cage that value belongs to.

Arguments carry their own cage IDs because the caller is not always the cage the syscall operates on. A grate forwarding a `write` on behalf of another cage passes a buffer pointer that refers to that cage's memory, and the per-argument cage ID tells RawPOSIX where to resolve it (see [cross-cage memory access](3i.md#cross-cage-memory-access) in the 3i docs).

Two conventions matter when reading or writing handlers:

- **Convert arguments early.** All values arrive as `u64`. A guest `-1` arrives as `18446744073709551615`, so handlers use the `typemap` conversion helpers (`sc_convert_sysarg_to_i32`, `sc_convert_buf`, and friends) at the top of the function before any logic runs.
- **Check unused slots.** Handlers assert that unused argument slots hold the expected sentinel via `sc_unusedarg` and panic on mismatch, treating unexpected values as a security violation.

## Source layout

```
src/rawposix/src/
├── lib.rs            crate root, module declarations
├── syscall_table.rs  syscall number → handler dispatch table
├── init.rs           rawposix_start/rawposix_shutdown, handler registration, RawCallFunc
├── fs_calls.rs       file, directory, memory, and shared-memory syscalls
├── net_calls.rs      socket and I/O-multiplexing syscalls
└── sys_calls.rs      process, identity, and signal syscalls
```

RawPOSIX depends on several sibling crates that used to be part of a single codebase and are now separate:

| Crate | Role |
|-------|------|
| `src/cage` | The `Cage` struct, cage table, `vmmap`, signal state |
| `src/fdtables` | Per-cage virtual fd to kernel fd translation |
| `src/threei` | The 3i dispatcher that routes syscalls to RawPOSIX handlers |
| `src/typemap` | Argument conversion and validation helpers |
| `src/sysdefs` | Shared constants (syscall numbers, errnos, platform constants) |

## Testing

RawPOSIX has no in-crate unit test suite. It is exercised end to end: C test programs in `tests/unit-tests/` are compiled to WebAssembly and run under `lind-boot`, so every syscall they make flows through the full glibc → Wasmtime → 3i → RawPOSIX path described above. The harness is `scripts/test/harnesses/wasmtestreport.py`, which compiles and runs each test and compares output against native execution or an `expected/` directory. See [Testing](../contribute/testing.md) for usage.

Logic that lives in the sibling crates is unit-tested there; for example, the `vmmap` implementation has Rust tests runnable with `cargo test` in `src/cage`.
