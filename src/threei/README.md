# 3i (ThreeI) Interface 

## Overview
This module describes the intercage interposition interface -- 3i (also pronounced "Three eye"), the means by which system calls and other inter-cage calls are routed for the Lind project.  3i enables call customization and system call filtering without modifying the source code of kernels or microkernels. It achieves this by providing a syscall table with customized jump endpoints for each cage/grate.

To motivate the need for 3i, consider the desire to add functionality like a new file system, perform system call tracing, or filter system calls in a fine grained manner.   In traditional Linux, to perform these actions one needs to either modify the kernel, use a mechanism like ptrace (which relays calls through the kernel), or similar.  3i enables the call dispatcher to directly route calls between grates or the underlying microvisor, providing increased speed, performance, and functionality.   Importantly, newly added functionality is all external to the microvisor.

To encapsulate external functionality, a cage may interpose on system calls of one or more cages.   We call such a cage a **grate** to signify it also changes, filters, blocks, etc. system calls. By using the grate, 3i addresses the challenge of monitoring system calls between cages without modifying kernel/microkernel code. This ensures kernel/microkernel integrity and compatibility while effectively monitoring system call jump addresses. Moreover, grate modifies essential system calls (such as socket create, bind, etc.) as needed to gather necessary information. This achieves stable and sustainable operations while minimizing performance impact from monitoring.

Compared to approaches that increase system call monitoring by frequently modifying kernel code or monitoring entire process's activities, 3i implements high-granularity monitoring of system calls externally to the kernel. This flexibility avoids exposing unnecessary information, reducing risks of information leakage, and prevents overall security vulnerabilities stemming from kernel/microkernel crashes.

Note also, that 3i can be used for other inter-cage calls, like RPC.   This document will be expanded in the future as this use case is further developed.

3i converts file system calls and RPC system calls into userspace function calls, thereby reducing additional overhead and security risks associated with frequent kernel interactions.

### 3i Function Calls 

|           Caller           |       Callee     |           Function            |  Interposable | Remarks |
|----------------------------|------------------|-------------------------------|---------------|---------------|
|           grate            |         3i       | `register_handler`            |      Yes      |Register a handler for a syscall|
|           grate            |         3i       | `copy_handler_table_to_cage`  |      Yes      |Overwrites the entire syscall handler table of a cage|
|           grate            |         3i       | `copy_data_between_cages`     |      Yes      |Copies memory across cages|
|           grate            |         3i       | `make_syscall`                |      No       |Call the registered handler for a syscall|
|   WASM / NaCl / RawPOSIX   |         3i       | `trigger_harsh_cage_exit`     |      No       |Kill a cage: See detailed explanation below|
|         3i / grate         | grate / RawPOSIX | `harsh_cage_exit`             |      Yes      |Notify a cage was killed: See detailed explaination below|

*NOTE: Interposable in the table means whether these calls are made via the system call table and thus whether or not a grate could alter their behavior*

#### `trigger_harsh_cage_exit` and `harsh_cage_exit`  

This is essentially a way for grates to clean up if a cage was abruptly killed (perhaps due to a signal).  `trigger_harsh_cage_exit` is triggered by the caging or signaling infrastructure to indicate that a cage will (uncleanly) exit. After receiving notification, 3i will cleanup the 3i data structure (which is the system call table) and then 3i will go through the respective grates until reaching 3i's version of the call by triggering `harsh_cage_exit`. This call can be thought of as notifying the grates and microvisor of the harsh exit of a program whose memory state cannot be relied upon. This is unlike the `exit_syscall`, which is performed by a functioning program with intact memory as part of its termination.

## Build and Testing

This project currently supports two implementations of the handler table:
- hashmap (default)
Uses `Mutex<HashMap<..>>`.

- dashmap (optional)
Uses `DashMap<..>`.

### How to build

By default, the `hashmap` implementation is used:

```sh
cargo build
```

To enable the `dashmap` backend:

```sh
cargo build --features dashmap
```

### How to test

Using default `hashmap`:
```sh
cargo test
```

Using `dashmap`:
```sh
cargo test --no-default-features --features dashmap
```
