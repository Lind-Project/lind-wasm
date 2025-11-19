# Lind

## Welcome to Lind!

Lind is a single-process sandbox that provides an option to safely execute programs. Lind executes applications using software fault isolation and a kernel microvisor to limit the potential of reaching bugs or security flaws in the application.

In Old Norse, Old High German and Old English a “lind” is a shield constructed with two layers of linden wood. Linden wood shields are lightweight, and do not split easily, an appropriate metaphor for a sandboxing system which employs two technologies.

# lind-wasm

`lind-wasm` is a WebAssembly-focused extension of the Lind project. It integrates multiple components—both in-house and third-party—to enable execution of POSIX-like applications in WebAssembly runtimes, such as Wasmtime.


## Getting started

Check out the [Getting started](https://lind-project.github.io/lind-wasm/getting-started/)
guide for a Hello World! example and [our docs](https://lind-project.github.io/lind-wasm/)
to learn more about Lind!



## Repository Structure and Components

This monorepo combines various subprojects and dependencies that work together to support Lind's goals. Below is an overview of the major components:

### In-House Projects

| Component     | Location          | Description                                                                 |
|---------------|-------------------|-----------------------------------------------------------------------------|
| `fdtables`    | `src/fdtables`    | Provides file descriptor table management, used to emulate POSIX semantics |
| `rawposix`    | `src/rawposix`    | Implementation of raw POSIX syscall wrappers used internally by Lind       |
| `threei`    | `src/threei`   | System call mediation layer for policy deployment                          |
| `typemap`   | `src/typemap`  | Defines custom data structures and type conversion functions used across Lind |
| `cage`      | `src/cage`     | Implements the custom `Cage` structure and its subsystems, including `vmmap` (virtual memory mapping) and `signal` handling |
| `sysdefs`     | `src/sysdefs`     | Shared system call definitions and constants for cross-platform support    |

### Third-Party Projects (Source)

| Project       | Location          | Description                                                                 |
|---------------|-------------------|-----------------------------------------------------------------------------|
| `glibc`       | `third_party/glibc` | Modified version of glibc to support WebAssembly and Lind interfaces       |
| `wasmtime`    | `src/wasmtime`    | Embedded Wasmtime runtime for running and debugging Lind-Wasm modules      |

### Third-Party Binaries

| Tool          | Location           | Description                                                                |
|---------------|--------------------|----------------------------------------------------------------------------|
| `binaryen`    | `tools/binaryen`   | Provides `wasm-opt` and other utilities used for optimizing wasm binaries |

---

