---
id: RawPOSIX
---

# Introduction

### Overview of RawPOSIX

RawPOSIX is a critical component of the Lind Project, designed to provide a POSIX-compliant interface for applications running within a microvisor environment. The primary goal of RawPOSIX is to enable the execution of both legacy and modern multi-processes’ applications safely and efficiently within the same address space without any modification to source code and perform the same behavior with applications running on native Linux.


### Purpose and Scope of RawPOSIX

The purpose of the RawPOSIX project is to provide an in-process OS while isolating them. By offering a POSIX-like interface, RawPOSIX is an interface implemented on top of standard POSIX (Linux) system calls. It provides functionalities such as signals, fork/exec, threading, file system operations, and networking. Additionally, RawPOSIX manages file descriptors (FDs), threads, and other resources independently for each cage, ensuring proper isolation and resource handling. This is particularly beneficial for legacy applications that rely on POSIX standards.

The scope of RawPOSIX encompasses several key areas:
- **System Call API:** Implementing a set of raw POSIX system calls that redirect low level operations to kernel and a set of userspace system calls based on POSIX standard to cover process management, network operations, and memory management.
- **Cage Structure:** A "cage" data structure in RawPOSIX is designed to handle per-process information while providing required memory management.
- **Testing and Validation:** Providing a testing framework to ensure the reliability and correctness of the RawPOSIX implementation.


### Key Components and Files

The RawPOSIX repository is organized into several important folders and files that contribute to its functionality:
- **src/:** This directory contains the main Rust codebase for RawPOSIX. It includes the implementation of the syscall API and other core components.
- **syscalls.rs:** This file defines the various system calls supported by RawPOSIX, implementing the logic for each operation.
- **cage.rs:** This file contains definitions of the Cage data structure as well as functions of corresponding operations like creation / insertion / etc. and life cycle management of cages.
- **tests/:** This directory includes test cases and scripts to validate the functionality of RawPOSIX. It ensures that all system calls and cage operations work as expected.
- **docs/:** Documentation files that provide additional context and instructions for setting up and using RawPOSIX.


# Syscall API

### Supported System Calls

In RawPOSIX, raw syscalls are used for direct interactions with the Linux kernel to handle low-level operations, while userspace syscalls serve as abstractions tailored to manage runtime-specific needs (e.g., WASM or Native Client) and ensure isolation through features like per-cage memory management and multi-processing support.

For standard system calls, RawPOSIX primarily processes variables passed from the runtime environment and redirects them to the Linux kernel. Beyond supporting standard POSIX system calls (e.g., file system and networking calls), RawPOSIX implements additional features, including:
- **Memory Management:** RawPOSIX provides memory management tailored to the runtime environment, leveraging VMMap-related system calls to enable per-cage memory management.
- **Process Management:** Functions such as wait, waitpid, fork, exec, and signal handling are implemented to support multi-processing. These functions update the cage structure and corresponding data structures as needed, ensuring proper isolation and accurately reflecting the state of processes.


# Testing

RawPOSIX employs a comprehensive testing framework to validate its functionality and ensure that all components operate as expected. The testing framework is designed to cover scenarios including both normal usage and error returns. Tests can be found on: lind-wasm/src/RawPOSIX/src/tests/
