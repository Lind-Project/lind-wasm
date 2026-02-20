# Grates

A grate is a cage whose primary role is to intercept and handle system calls issued by other cages. Lind makes no architectural distinction between cages and grates; any cage may register system call handlers and thereby act as a grate.

Grates allow policy and system services to be implemented outside the trusted runtime, without kernel modifications or special privileges.

## Why grates exist

In traditional Linux systems, extending or intercepting system calls typically requires kernel modifications, kernel modules, or mechanisms such as eBPF. These approaches are privileged, restricted in what they can safely do, and difficult to compose into larger systems.

Grates allow this functionality to be implemented entirely in user space. Because they are ordinary cages, grates can implement services that are impractical or impossible to build using kernel hooks alone, such as an in-memory filesystem, custom networking stacks, or rich virtualization layers, while remaining outside the trusted runtime.

3i makes this possible by allowing cages to register handlers for other cages’ system calls. Since writing such interception logic is lightweight and common, Lind gives these cages the special name “grates.”

## Inheritance properties

When a cage forks, the child inherits the parent’s system call handler table.

Inheritance is a fundamental property of Unix process semantics. In Linux, a child process inherits open file descriptors, signal handlers, credentials, and namespace membership. This ensures that the child continues execution within the same environment and policy context as the parent.

In Lind, the system call handler table is part of that execution context. If Cage A is subject to a namespace grate or policy grate, a forked child must remain subject to the same routing structure. Without handler inheritance, the child would execute with a different routing configuration and could bypass intended behavior.

3i implements this using `copy_handler_table_to_cage`. During `fork`, the parent’s handler table is copied to the newly created cage so that the child begins with identical routing behavior.

In addition to inheritance across fork, an ancestor grate may modify the system call tables of its descendants. This capability is used in several patterns, including clamping, but is not limited to it. It allows structural control over how routing evolves as new cages are created.

## Cross-cage buffers

3i allows system call arguments to specify which cage owns a referenced buffer. This enables grates to safely inspect, modify, or forward memory arguments without unnecessary copying.

For example, if Cage A calls `write` and passes a pointer to a buffer, a grate can explicitly reference that buffer as belonging to A. This allows the grate to examine or adjust the data before forwarding the call, without incorrectly accessing its own memory space.

## Acting on behalf of other cages

A grate may perform system calls on behalf of another cage so that the system behaves as though the originating cage made the call.

Suppose Cage A invokes `fork`, and the call is intercepted by Grate G. If G simply executes `fork` using its own identity, then G, not A, would be duplicated. This would break process semantics.

Instead, G issues `make_syscall` and specifies Cage A as the target cage. The new process state is therefore associated with A, not G.

Similarly, if Cage A invokes `mmap` and Grate G modifies the arguments before forwarding the call, the resulting memory mapping must be installed in A’s address space rather than G’s. By specifying the target cage explicitly, G ensures that the operation affects A’s state rather than its own.

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

Stacking is the most common form of grate composition. Grates are arranged in a linear chain, and system calls flow through them sequentially. This is analogous to how output flows through a Unix pipeline from one process’s stdout to another process’s stdin. In a Unix pipeline, a program may log or observe the input, modify it, filter it, or block it entirely before passing it along. Changing the order of commands changes the overall behavior.

Similarly, a grate may log or observe a system call and forward it (for example, like `strace`), modify it before forwarding (such as a file encryption grate), block it and return an error (similar to `seccomp`), or replace it with different system calls (for example, implementing a network filesystem). Each grate acts independently, and the overall behavior emerges from how the grates are composed.

## Clamping

Clamping is used when an ancestor grate needs structural control over how a descendant handles system calls.

Often this is used to divide a namespace, but it is more general than that. Clamping allows an ancestor to ensure that certain checks, routing decisions, or transformations occur before a descendant’s handler executes.

For example:

- A filesystem namespace grate may route `/repo` to an in-memory filesystem grate while routing `/out` to the host filesystem.
- A networking namespace grate may examine the destination of `send` and route traffic for certain IP and port combinations to different networking grates.
- A validation grate may enforce argument checks before allowing a downstream service grate to execute.

In these cases, the application still invokes the same system call, such as `write`.

However, the ancestor grate may register additional internal syscall numbers. For example, a namespace grate may allow the normal `write` syscall number to continue routing toward RawPOSIX or a grate stacked below it, while registering a new internal syscall number such as `write_imfs` for an in-memory filesystem grate.

When `write` is invoked by a child cage, the ancestor grate examines the arguments and decides how to route the call. It may issue a new 3i call using the original `write` number to continue normal routing, or issue a call using the internal `write_imfs` number to direct the operation to the in-memory filesystem grate.

Clamping is made possible by interposing on `register_handler`. When a descendant grate attempts to install a handler for a child cage, the ancestor intercepts that registration and substitutes its own handler instead.

As a result, system calls issued by the child are always routed through the ancestor first. The ancestor may perform checks, modify arguments, or make routing decisions before issuing a new 3i call to the appropriate downstream grate or to RawPOSIX, which executes the system call against the host kernel.