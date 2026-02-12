# Grates

A grate is a cage whose primary role is to intercept and handle system calls issued by other cages. Lind makes no distinction between cages and grates; any cage may register system call handlers and thereby act as a grate.

Grates allow policy and system services to be implemented outside the trusted runtime, without kernel modifications or special privileges.

## Why grates exist

3i enables cages to intercept system calls from other cages. Because writing such interception logic is lightweight and common, Lind gives these cages the special name “grates.”

This design allows functionality such as logging, filtering, virtualization, and service implementation to be expressed as ordinary user-space programs.

## Composition

Grates are composable. A grate may itself have another grate beneath it that provides additional functionality. This mirrors the Unix philosophy of building complex behavior from small, composable components.

In practice, grates are composed using two patterns: stacking and clamping.

## Stacking

Stacking is the most common form of grate composition. Grates are arranged in a linear chain, and system calls flow through them sequentially.  This is analogous to how output flows through a Unix pipeline from one process's stdio to another's stdout.  Note that in a Unix pipeline, the output is often transformed in the process, including creating new emissions to stderr or other files.  Similarly, a grate may pass system calls below (e.g., strace), may transform them (e.g., a file encryption grate), omit them (e.g., seccomp), perform different calls (e.g., a network filesystem), etc. in whatever means it has been designed to do.

When a system call reaches a grate in a stack, the grate may choose one of the following behaviors:

1. **Intercept, handle, and pass down**  
   The grate performs some action and then issues a new 3i call so the system call continues to the next handler.  
   This is analogous to tracing or logging tools that observe calls without changing semantics.

2. **Intercept and handle without passing down**  
   The grate fully handles the system call and returns a result to the caller without forwarding it further.  
   This is common for virtualization or user-space service implementations.

3. **Not intercept**  
   The grate does not register a handler for the system call. The call bypasses the grate and is routed by 3i to the next handler.

4. **Block**  
   The grate intercepts the system call and denies it, returning an error without forwarding it further.  
   This is analogous to policy enforcement mechanisms such as seccomp.

Each grate makes its decision independently. The overall behavior emerges from their composition.

## Clamping

Clamping is used when an ancestor grate wants to enforce a specific syscall routing structure on its descendants.

The key distinction between stacking and clamping is when interposition occurs:

- Stacking interposes at system call execution time.
- Clamping interposes at system call registration time.

In clamping, an ancestor grate interposes on calls to `register_handler` made by a descendant. When a descendant attempts to install a handler for a child cage, the ancestor substitutes its own handler instead.

As a result, system calls issued by the child are always routed to the ancestor grate first.

When a system call reaches the ancestor grate, it issues a new 3i call. The call is then routed according to the ancestor’s policy, which may direct it to:
- a service grate
- another grate further down the stack
- or eventually to RawPOSIX via normal 3i routing

Service grates do not receive system calls directly from applications; they are invoked only when explicitly chosen by the ancestor grate.

## Acting on behalf of other cages

A grate may perform system calls on behalf of another cage so that the system behaves as though the originating cage made the call. This is required for correct semantics in operations such as process creation and memory management.

## Cross-cage buffers

3i allows system call arguments to specify which cage owns a referenced buffer. This enables grates to safely inspect, modify, or forward memory arguments without unnecessary copying.

## Inheritance properties

- A child inherits its parent’s system call handlers on fork.
- An ancestor may modify the system call tables of its descendants and perform calls on their behalf.
