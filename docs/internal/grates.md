# Grates

A grate is a cage whose primary role is to intercept and handle system calls issued by other cages. Lind makes no architectural distinction between cages and grates; any cage may register system call handlers and thereby act as a grate.  However, legacy programs will of course be unaware of these system calls and so will not make them.

Grates allow policy and system services to be implemented outside the trusted runtime, without kernel modifications or special privileges.

## Why grates exist

In traditional Linux systems, extending or intercepting system calls typically requires kernel modifications, kernel modules, or mechanisms such as eBPF. These approaches are privileged, restricted in what they can safely do, and difficult to compose into larger systems.

Grates allow this functionality to be implemented entirely in user space. Because they are ordinary cages, grates can implement services that are impractical or impossible to build using kernel hooks alone, such as an in-memory filesystem, custom networking stacks, or rich virtualization layers, while remaining outside the trusted runtime.

3i makes this possible by allowing cages to register handlers for other cages' system calls. Since writing such interception logic is lightweight and common, Lind gives these cages the special name "grates."

## Inheritance properties

When a cage forks, the child inherits the parent's system call handler table.

Inheritance is a fundamental property of Unix process semantics. In Linux, a child process inherits open file descriptors, signal handlers, credentials, and namespace membership. This ensures that the child continues execution within the same environment and policy context as the parent.

In Lind, the system call handler table is part of that execution context. If Cage A is subject to a namespace grate or policy grate, a forked child must remain subject to the same routing structure. Without handler inheritance, the child would execute with a different routing configuration and could bypass intended behavior.

3i implements this using `copy_handler_table_to_cage`. During `fork`, the parent's handler table is copied to the newly created cage so that the child begins with identical routing behavior.

In addition to inheritance across fork, an ancestor grate may modify the system call tables of its descendants. This capability is used in several patterns, including clamping, but is not limited to it. It allows structural control over how routing evolves as new cages are created.

## Cross-cage buffers

3i allows system call arguments to specify which cage owns a referenced buffer. This enables grates to safely inspect, modify, or forward memory arguments without unnecessary copying.

For example, if Cage A calls `write` and passes a pointer to a buffer, a grate can explicitly reference that buffer as belonging to A. This allows the grate to examine or adjust the data before forwarding the call, without incorrectly accessing its own memory space.

## Acting on behalf of other cages

A grate may perform system calls on behalf of another cage so that the system behaves as though the originating cage made the call.

Suppose Cage A invokes `fork`, and the call is intercepted by Grate G. If G simply executes `fork` using its own identity, then G, not A, would be duplicated. This would break process semantics.

Instead, G issues `make_syscall` to invoke `fork`, specifying Cage A as the target cage. The new process state is therefore associated with A, not G.

Similarly, if Cage A invokes `mmap` and Grate G modifies the arguments before forwarding the call, the resulting memory mapping must be installed in A's address space rather than G's. By specifying the target cage explicitly, G ensures that the operation affects A's state rather than its own.

This mechanism allows grates to interpose on system calls while preserving correct POSIX behavior.

## Composability

Grates are composable. A grate may itself have another grate beneath it that provides additional functionality. This mirrors the Unix philosophy of building complex behavior from small, focused components.

In Unix, programs are often composed using pipelines.

For example:

```sh
find . -name "*.log" | grep error | sort
```

This command:
- finds all `.log` files,
- filters them to those containing the word "error",
- and sorts the matching paths.

Reordering the commands changes behavior. For example:

```sh
find . -name "*.log" | sort | grep error
```

Now the file paths are sorted before filtering. In larger pipelines, changing the order can significantly change semantics or performance.

Grates follow the same pattern. Each grate performs a specific function, and system calls flow through them in sequence. The overall behavior depends on how the grates are arranged.

In practice, grates are composed using two patterns: stacking and clamping.

## Stacking

Stacking is the most common form of grate composition. Grates are arranged in a linear chain, and system calls flow through them sequentially. This is analogous to how output flows through a Unix pipeline from one process's stdout to another process's stdin. In a Unix pipeline, a program may log or observe the input, modify it, filter it, or block it entirely before passing it along. Changing the order of commands changes the overall behavior.

Similarly, a grate may log or observe a system call and forward it (for example, like `strace`), modify it before forwarding (such as a file encryption grate), block it and return an error (similar to `seccomp`), or replace it with different system calls (for example, implementing a network filesystem). Each grate acts independently, and the overall behavior emerges from how the grates are composed.

For example:

```
lind strace-grate -- clang hello.c -o hello
```

Here, `clang` executes as an application cage. When it issues system calls, they flow first through the strace grate, which logs each call and forwards it onward. The call then continues to RawPOSIX, which executes it against the host kernel. The strace grate observes but does not modify or block the call.

## Clamping

Clamping is a composition mechanism that allows a grate to selectively route system calls to other grates based on some condition. Rather than all calls flowing through every grate in the stack unconditionally (as with stacking), a clamping grate evaluates a routing rule and only sends matching calls through the clamped grates. Non-matching calls bypass them entirely.

For example:

```
lind namespace-grate --prefix /tmp %{ imfs-grate %} python
```

Here, the namespace grate routes filesystem calls conditionally. Paths under `/tmp` are routed to the IMFS grate, which implements an in-memory filesystem. Other paths skip IMFS entirely and continue through normal routing toward RawPOSIX. The `%{` and `%}` delimiters mark the boundary of the clamp on the command line, indicating which grates are conditionally applied.

This reads as a conditional stack:

```
python
if --prefix /tmp
    imfs-grate
endif
```

Clamping is made possible by interposing on 3i operations such as `register_handler` and `exec`. When a clamped grate attempts to register a handler for a system call, the clamping grate intercepts that registration, installs itself as the handler, and sets up a forwarding path to the clamped grate under an internal system call number. This ensures that the clamping grate remains in the routing path and can evaluate its condition before dispatching. Clamps can be nested, placed in series, or combined with unconditional stacking.

The full mechanism, including command-line syntax, exec and register_handler interposition, fd table management, and worked examples, is described in [Clamping](clamping.md).