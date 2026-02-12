# Grates

A grate is a cage whose primary role is to intercept and handle system calls issued by other cages. Lind makes no distinction between cages and grates; any cage may register system call handlers and thereby act as a grate.

Grates allow policy and system services to be implemented outside the trusted runtime, without kernel modifications or special privileges.

## Why grates exist

In traditional Linux systems, extending or intercepting system calls typically requires kernel modifications, kernel modules, or mechanisms such as eBPF. These approaches are privileged, restricted in what they can safely do, and difficult to compose into larger systems.

Grates allow this functionality to be implemented entirely in user space. Because they are ordinary cages, grates can implement services that are impractical or impossible to build using kernel hooks alone — such as an in-memory filesystem, custom networking stacks, or rich virtualization layers — while remaining outside the trusted runtime.

3i makes this possible by allowing cages to register handlers for other cages’ system calls. Since writing such interception logic is lightweight and common, Lind gives these cages the special name “grates.”

## Composability

Grates are composable. A grate may itself have another grate beneath it that provides additional functionality. This mirrors the Unix philosophy of building complex behavior from small, composable components.

In practice, grates are composed using two patterns: stacking and clamping.

## Stacking

Stacking is the most common form of grate composition. Grates are arranged in a linear chain, and system calls flow through them sequentially.  This is analogous to how output flows through a Unix pipeline from one process's stdio to another's stdout.  Note that in a Unix pipeline, the output is often transformed in the process, including creating new emissions to stderr or other files.  Similarly, a grate may pass system calls below (e.g., strace), may transform them (e.g., a file encryption grate), omit them (e.g., seccomp), perform different calls (e.g., a network filesystem), etc. in whatever means it has been designed to do.

Each grate independently decides how to handle the system call before optionally issuing a new 3i call to continue routing. The overall behavior emerges from their composition.

## Clamping

Clamping is used when an ancestor grate needs to divide a resource namespace so that a single system call may be routed to different implementations depending on policy.

For example:

- A filesystem namespace grate may route `/repo` to an in-memory filesystem grate while routing `/out` to the host filesystem.
- A networking namespace grate may examine the destination of `send` and route traffic for certain IP/port combinations to one networking grate and others to a different one.
- A time namespace grate may virtualize clocks for some cages while allowing others to observe host time.

In these cases, the original system call interface is preserved for the application — for example, the application still invokes `write`.

However, the ancestor grate may register additional internal syscall numbers. For example, a namespace grate may allow the normal `write` syscall number to continue routing toward RawPOSIX (or a grate stacked below it), while registering a new internal syscall number such as `write_imfs` for an in-memory filesystem grate.

When `write` is invoked by a child cage, the namespace grate examines the arguments (such as the file descriptor or path) and decides how to route the call. It may issue a new 3i call using the original `write` number to continue normal routing, or issue a call using the internal `write_imfs` number to direct the operation to the in-memory filesystem grate.

Clamping allows a grate to divide a namespace while preserving the original application-visible syscall interface.


## Acting on behalf of other cages

A grate may perform system calls on behalf of another cage so that the system behaves as though the originating cage made the call.

For example, if a cage invokes `fork` and a grate simply performs `fork` using its own identity, the grate — not the originating cage — would be duplicated. Instead, the grate must issue `make_syscall` specifying the original cage as the target so that the new process state is associated with the correct cage.

Similarly, if a grate interposes on `mmap`, it may need to ensure that the resulting memory mapping is installed in the calling cage’s address space rather than its own.

By allowing a grate to specify the target cage for a system call, 3i preserves POSIX semantics while still enabling interposition.

## Cross-cage buffers

3i allows system call arguments to specify which cage owns a referenced buffer. This enables grates to safely inspect, modify, or forward memory arguments without unnecessary copying.

## Inheritance properties

- A child inherits its parent’s system call handlers on fork.
- An ancestor may modify the system call tables of its descendants and perform calls on their behalf.
