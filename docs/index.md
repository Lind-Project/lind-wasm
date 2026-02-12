---
id: Overview
---

# Lind-Wasm

Lind is a sandboxing system that runs multiple mutually untrusted applications within a single, unprivileged Linux process. Each application executes in an isolated execution context, called a cage, with its own memory, control flow, and system call behavior.

Unlike traditional process isolation, Lind provides strong intra-process isolation while preserving POSIX semantics and avoiding kernel modifications or privileged execution.

In Old Norse, Old High German, and Old English, a “lind” is a shield constructed with two layers of linden wood. Linden wood shields are lightweight and resistant to splitting — an appropriate metaphor for a sandboxing system built from layered isolation technologies.

Lind-Wasm is a realization of Lind that uses WebAssembly for software fault isolation and a small trusted runtime to enforce isolation and mediate system calls.

## Technology Overview

### Cages

A cage is an isolated execution context within the Lind process. Conceptually, a cage is similar to a Linux process, but multiple cages coexist within a single host process.

Applications are recompiled to WebAssembly and linked against a modified glibc so that all system calls are issued through 3i. Most applications require no source-level changes.

Each cage has:
- isolated memory and control flow
- its own system call routing configuration
- POSIX-like process and thread semantics

### Grates

A grate is a cage whose primary purpose is to intercept and handle system calls issued by other cages. Lind makes no distinction between cages and grates at the mechanism level; any cage may act as a grate by registering system call handlers.

Grates are commonly used to implement policy and system services outside the trusted runtime. They are lightweight, composable, and run entirely in user space.

### 3i

The Intercage Interposition Interface (3i) provides a programmable system call routing mechanism between cages, grates, and the microvisor.

Each cage has its own system call table, which may route system calls to:
- another cage acting as a grate
- or the microvisor

3i enables interception, delegation, filtering, and mediation of system calls without modifying kernel code.

### Microvisor (RawPOSIX)

RawPOSIX is a small, trusted runtime component that provides POSIX-compatible system call implementations on behalf of cages. It runs entirely within the unprivileged Lind process and is responsible for interacting with the host kernel.

## Components

### Wasmtime

Lind-Wasm uses [Wasmtime](https://github.com/bytecodealliance/wasmtime) as the WebAssembly runtime responsible for executing cages. Wasmtime provides fast execution, memory isolation, and a well-defined embedding API.

### lind-glibc

Lind includes a modified glibc that can be compiled to WebAssembly. The modifications remove architecture-specific assembly and redirect system calls through 3i using a uniform 64-bit calling convention.

### 3i Implementation

The 3i implementation provides the core system call routing and interposition mechanism used by both application cages and grates.
