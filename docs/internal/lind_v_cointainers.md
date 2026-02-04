# Lind vs. Containers: Taking the OS Where It Can't Go
While Lind shares the goal of isolation with traditional container systems (like Docker or LXC), the architectural approach—and the resulting capabilities—are fundamentally different.

1. The Kernel Dependency vs. The Portable Microvisor
- Containers (Shared Kernel): Traditional containers are essentially namespaces within the host operating system's kernel. They rely entirely on the host's POSIX kernel to function.  Docker won't run well  on a system without a compatible POSIX  kernel (like a web browser or a microcontroller), it simply cannot run.
- Lind (Kernel Inception): Lind utilizes a Microvisor, which is a small, POSIX-compliant kernel running within a process. Because the "operating system" is just a software component (compiled to WebAssembly), Lind carries its own kernel with it. This allows Lind to run fully isolated POSIX applications in environments that have no native kernel access, such as inside a web browser or on edge devices, using runtimes like Wasmtime.

2. Rigid Configuration vs. Programmable Interposition
- Containers: Modifying the behavior of a container typically involves static configuration (Dockerfiles) or coarse-grained security profiles (`seccomp`). Intercepting specific system calls to change application behavior is difficult and performant-heavy.
- Lind: Lind’s **Grates** allow for programmable, fine-grained interposition on system calls. Because the "system calls" are just function jumps, developers can dynamically rewrite the application's reality (as seen in the "Ransomware Aquarium" example) with zero kernel overhead.

3. Isolation Depth
- Containers: Rely on the host kernel's ability to enforce barriers. A vulnerability in the host kernel compromises all containers.
- Lind: Uses Software Fault Isolation (SFI) and memory safety provided by the WebAssembly sandbox. The "Kernel" (Microvisor) is distinct from the host, adding a robust layer of defense-in-depth. If a Cage crashes the Microvisor, it only crashes that single Lind process, not the host machine.

## Summary
If a Container is a "partitioned room" in a shared house, Lind is a "portable building" that can be dropped anywhere—even onto a different planet (the browser)—and it will still function exactly like a standard POSIX environment.
