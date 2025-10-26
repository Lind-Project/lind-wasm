---
id: Overview
---

# Lind-Wasm

Lind is a single-process sandbox that provides an option to safely execute programs. Lind executes applications using software fault isolation and a kernel microvisor to limit the potential of reaching bugs or security flaws in the application.

In Old Norse, Old High German and Old English a "lind" is a shield constructed with two layers of linden wood. Linden wood shields are lightweight, and do not split easily, an appropriate metaphor for a sandboxing system which employs two technologies.

## Core Concepts

- **Cage**: Lightweight isolation boundary within a process
    - Can run legacy code (may need recompilation)
    - Protects and isolates memory
- **Microvisor**: Small POSIX compliant kernel within a process
    - Provides a POSIX interface
    - Distinct isolation between cages
- **3i (three eye)**: Capability-based POSIX interfaces between cages

## Technology Overview

### Cages
Memory and bookkeeping that encapsulates the idea of a typical OS process, encompassing applications as well as grates.

### Grates
Provide services to descendant cages by performing trusted operations on their behalf, without requiring additional code in the microvisor's TCB. 

Grates can run arbitrary code, with restrictions only placed by grates beneath them.
The microvisor itself implements a grate with privileged access to call into the Linux kernel.

**Inheritance Properties**:

- A child inherits system call handlers from parent on fork
- If cage A was forked by cage B, cage A will have the same system call handlers as cage B
- If grate A was forkinterpose()'d by grate B, grate A inherits B's system call behavior changes

### 3i System
The 3i system serves as:

- Central point for all cage communication
- Table container for system call routing
- Security control mechanism for system call interception
- Privilege management system for blocking unnecessary calls

## Components

### Wasmtime
Wasmtime is a fast and secure runtime for WebAssembly designed by Bytecode Alliance. Lind-wasm uses wasmtime as a runtime with added support for multi-processing via Asyncify.

### lind-glibc

We’ve ported glibc so that it can be compiled to wasm bytecode and linked with any wasm binary. This includes minor changes like replacing assembly code, and add a mechansim to transfer system calls to the trusted runtime and microvisor.

### RawPOSIX
Provides normal POSIX system calls including:

- Signals
- Fork/exec
- Threading
- File system
- Networking
- Separate handling of cages' fds and threads

### 3i Implementation
The iPC (intra-process call) interposable interface enables secure and efficient cage communication with function call-like speed. It provides POSIX interfaces between cages with interposition capabilities, enabling fine-grained security and access control while maintaining program behavior.
