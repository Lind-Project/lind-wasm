---
id: Overview
---

# Lind-Wasm

Lind is a sandbox that isolates different applications in the same address space. Thus, conceptually, it executes different applications (which traditionally would be different processes) in separate parts of a single address space, under a single, non-privileged Linux process.   To provide memory safety, control flow integrity, memory isolation, and similar properties, this version of Lind executes applications using WebAssembly for software fault isolation.  Lind also contains a custom kernel microvisor, written in Rust, to limit the potential damage of bugs or security flaws in an application.

In Old Norse, Old High German and Old English a "lind" is a shield constructed with two layers of linden wood. Linden wood shields are lightweight, and do not split easily, an appropriate metaphor for a sandboxing system which is lightweight and which provides layered security.

## Core Concepts

- **Cage**: This term describes the isolated memory namespace that an application executes in.  It is analogous to a process in Linux. 
    - Can run legacy code compiled with Wasm as a target
    - Protects and isolates memory, control flow, etc.
- **Microvisor**: RawPOSIX is a small kernel running within the Lind process.  This is analogous to the Linux kernel.  Note, however, that this and all of the rest of Lind runs as an unprivileged Linux process.
    - Provides a POSIXish interface (runs most Linux programs)
    - Handles file descriptor separation, fork, exec, signals, threading, etc.
- **3i (three eye)**: Capability-based POSIX interface to call between cages or into the microvisor.  This is conceptually similar to a programmable system call table.
    - Each cage has a separate system call table which can be independently changed to redirect into other cages or the microvisor
    - Fast, isolated calling between cages
    - Enables complex functionality (system call filtering, file systems, proxies, etc.) to be external to the microvisor

## Technology Overview

### Cages
A cage is simply a Linux process.  The code must recompiled to run in Wasm and linked to our modified glibc so that it makes its system calls through 3i.  However, the source code for applications only needs to be modified in rare cases (such as applications that directly make system calls).

### Grates
A key advantage of 3i is the ability to support interposition.  In other words, a cage can intercept the system calls from another cage.  Because the use case of writing a cage to intercept system calls is very lightweight it is common in Lind.  As such, we give these cages the special name "grate".  This is meant to convey the mental picture of a caged application which calls down through a series of grates before (potentially) reaching the operating system.

Note that a grate is a cage and Lind makes no actual distinction between them.  Any cage can make the system calls available to grates (unless a grate below it prevents it).  It is just that most legacy applications do not need to regularly make such calls.  This is analogous to strace and its use of the ptrace mechanism.

A major advantage is that this means that implementing something like an in-memory file system can now be done without changing the microvisor or other trusted code.  A grate can intercept the file system calls and code written in C, Rust, etc. can be used to provide this functionality.  Similarly, a network file system can be implemented in a grate by the grate making whatever network system calls are needed.  

Another important feature of grates is that they are composable.  A grate may itself have another grate beneath it which provides a separate service.  The recommended grate creation philosophy of 3i is similar to the philosophy in Unix of having small, composable commands you combine with pipes and similar functionality.  So, grates tend to be smaller, simpler utilities that can be combined.  This is sensible since the overhead of having separate cages and calling between cages is very low.

Another similarity of Unix pipelines has to do with how grates interact.  The overwhelmingly common use case for pipes in Unix is to chain commands together sequentially so that the stdout of one becomes the stdin of the next.  However, the pipe functionality is itself general and supports many different use cases beyond this.  The interposition mechanism in 3i is similar in that most grates are likely to be stacked and simply provide functionality to whatever is above them.  However, since the system call table is per-cage and is truly programmable, a sufficiently privileged (ancestor) grate could rewire system calls for all of its descendents in any manner desired.

In addition to intercepting system calls, a grate can also perform system calls on behalf of a descendant cage.  This is useful in situations where a cage should perform an operation like exiting or setting up a memory mapping or similar, where the goal is for the system to act as though the cage is making the call instead of the grate.

When performing system calls, it is often useful for a grate to be able to pass arguments that refer to buffers in other cages (e.g., the buffer used in a write call).  Thus 3i system call arguments support a notion of which cage each argument comes from.

**Inheritance Properties**:

- A child inherits system call handlers from its parent on fork. Thus, if cage A was forked by cage B, cage A will have the same system call handlers as cage B
- An ancestor can change the system call table for its decendents and perform calls on their behalf.

## Components

### Wasmtime (our caging technology)
Wasmtime is a fast and secure runtime for WebAssembly designed by the Bytecode Alliance. Lind-wasm uses wasmtime as a runtime with added support for multi-processing via Asyncify.

### lind-glibc

We’ve ported glibc so that it can be compiled to wasm bytecode and linked with any wasm binary. This includes minor changes like replacing assembly code, and add a mechansim to transfer system calls to the trusted runtime and microvisor.  Also, all system calls are converted to 64-bit system call types because all grates (and the underlying RawPOSIX implementation) support 64-bit system calls.

### RawPOSIX (our microvisor technology)
Provides normal POSIX system calls including:

- Signals
- Fork
- Exec
- Threading
- File system
- Networking
- Separate handling of cages' fds and threads

### 3i Implementation
The intra-process interposable interface (3i) enables secure and efficient cage communication with close to function call-like speed. It provides POSIX interfaces between cages with interposition capabilities, enabling fine-grained security and access control by supporting the construction of grates.
