# lind-boot

lind-boot is the bootstrap and execution entry point for running POSIX-like WebAssembly programs on top of Lind, RawPOSIX, and Wasmtime. It is responsible for turning a WebAssembly module into a process-like entity that supports fork, exec, exit, multi-threading, and dynamic syscall interposition (3i), all implemented entirely in userspace. Rather than acting as a simple Wasm runner, lind-boot serves a role closer to a process runtime, coordinating Wasmtime, WASI, Lind, RawPOSIX, and 3i into a single coherent execution environment.

At a high level, lind-boot sits at the boundary between the command-line interface and the Lind-wasm runtime. It initializes the Wasmtime engine and store, configures WASI and thread support, brings up RawPOSIX, and establishes the necessary bridges that allow syscalls originating from WebAssembly to be dynamically routed, interposed, and re-entered through the Wasmtime runtime.

## Directory Layout

```text
src/
├── main.rs
├── cli.rs
├── clone.rs
├── exec.rs
├── exit.rs
├── shims.rs
├── lind_mpk/
└── lind_wasmtime/
    ├── mod.rs
    ├── execute.rs
    ├── host.rs
    └── trampoline.rs
```

`shims.rs` remains the active clone, exec, and exit dispatch. The new
`clone.rs`, `exec.rs`, and `exit.rs` modules are reserved for its eventual
replacement. The draft interface lives in
the [`lind-if`](../../lind-if) crate; the Wasm and MPK
implementations are scaffolded in [`lind-runtimes`](../../lind-runtimes).
Memory operations now follow
`RawPOSIX -> 3i -> shims.rs -> LindRuntime::do_mmap/do_mprotect/do_munmap`.
RawPOSIX keeps argument validation, fd/address translation, and Vmmap updates;
the runtime receives host addresses and host fds. The internal requests use
syscall numbers 9, 10, and 11 with the affected cage in `arg1_cageid`, separate
from the guest-facing RawPOSIX handlers. Results retain full-width addresses or
negative errnos. Runtime `do_munmap` is a plain host unmap. Guest `munmap` and
`brk` shrinking keep the runtime reservation because RawPOSIX itself requests a
fixed anonymous `PROT_NONE` `do_mmap` over the released range instead of
calling `do_munmap`. MPK protection-key management (`pkeys`) lives in
`lind-runtimes::lind_mpk`; MPK `do_mmap` uses it directly so fixed mappings
inside a reservation keep the cage's key. `lind_mpk::pkeys` here re-exports it,
and boot still installs its hooks into `cage` for legacy mapping paths.
Signals follow `Lind-Core -> LindRuntime::notify_signal`: `register_syscall_entries`
registers both runtimes with `cage` by runtime type (`cage::register_runtime`),
and `cage` depends only on `lind-if`, not on the implementations. Core queues
the signal and calls `lind_notify_signal` for every queued signal and again
when signals are unblocked. If Core's bookkeeping holds a deliverable signal,
the runtime marks the selected thread's epoch and interrupts it with `SIGUSR2`,
and the thread then dequeues the signal itself via `lind_get_first_signal`.
Notifications for still-blocked signals leave the thread undisturbed, so
blocking calls do not fail with `EINTR`.
Reservation bootstrap (`is_reservation = true`) and the other draft operations
remain unimplemented and return `ENOSYS`.

## How To Use

```sh
# Compile
cargo build

# Run program
./target/lind-boot [flags...] wasm_file.wasm arg1 arg2 ...
```

Supported flags:

```sh
    --verbose
    --debug
    --env NAME[=VAL]
```

## Design Overview

From the user’s perspective, lind-boot behaves like a conventional process launcher.

### main.rs

Execution begins in main.rs, where command-line arguments are parsed and passed to the core execution logic. The entry point accepts a WebAssembly binary followed by program arguments.

### host.rs

Host-side runtime state is encapsulated in `HostCtx`, defined in host.rs. This structure holds the WASI Preview1 context, the WASI threads context, and the Lind multi-process context. 

### execute.rs

The core logic lives in execute.rs. Here, lind-boot creates a Wasmtime engine and store, loads the WebAssembly module, and initializes the Lind and RawPOSIX subsystems. RawPOSIX is started before any user code runs, after which the initial cage is created and registered. A global VMContext pool is then initialized to support later re-entry into the Wasmtime runtime. At this stage, lind-boot also registers:

1) a unified trampoline function with 3i, allowing syscalls to be dynamically routed back into Wasmtime. 
2) raw wasmtime implementation function pointers for clone, exec, and exit 
3) function pointers for all RawPOSIX syscall implementation

Before execution begins, lind-boot attaches all required host-side APIs to the Wasmtime linker. This includes WASI Preview1 for argument and environment handling, WASI threads, and Lind-specific common and multi-process APIs.

Module instantiation occurs in `load_main_module`. The WebAssembly module is instantiated inside a Lind cage, after which the runtime checks for and invokes the `main` function because of our glibc modification. The main entry point is then resolved, stack bounds are initialized, and signal and epoch-related state is set up for the main thread of the cage. At this point, the WebAssembly program is fully initialized and starts running code logic.

One responsibility of lind-boot is capturing and managing Wasmtime’s internal `VMContext` pointers. After instantiation, lind-boot extracts the `VMContext` associated with the running instance and stores it in a global table indexed by cage ID. Additional backup instances are created to populate a pool of `VMContext`s that can be reused during grate calls and syscall re-entry. (See more comments on lind-wasm/src/wasmtime/crates/lind-3i)

### trampoline.rs

The re-entry mechanism is implemented in trampoline.rs. When 3i routes a syscall to a grate, it invokes a unified callback function registered by lind-boot. This trampoline retrieves the appropriate `VMContext` for the target cage, re-enters the Wasmtime runtime using `Caller::with`, and invokes a unified entry function inside the WebAssembly module. Control is then dispatched to the appropriate syscall implementation based on the function pointer originally registered with 3i. Once execution completes, the VMContext is returned to the global pool for future use.

