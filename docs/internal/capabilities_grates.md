# Grates and Capabilities in Lind-Wasm
## Introduction
Lind-Wasm utilizes a single-process sandbox approach involving Cages (isolation boundaries) and Grates (interposition layers) to execute programs safely. While Cages provide the memory isolation, Grates provide the control logic. Together, they implement a capability-based security model where access rights are explicitly managed rather than implicitly granted.

## Capabilities vs. Access Control Lists (ACLs)
To understand the architecture of Lind-Wasm, it is essential to distinguish between Access Control Lists (ACLs) and Capability-based security.
- **ACLs (Identity-Based)**: In traditional systems (like standard Unix file permissions), security is based on identity. The system checks "Who are you?" against a centralized list to determine if you are allowed to perform an action. This requires the kernel to mediate every check, often resulting in rigid, coarse-grained permissions.
- **Capabilities (Token-Based)**: In contrast, a capability model is based on possession. A "capability" is an unforgeable token (like a key or a specific function pointer) that inherently grants the right to perform an action. If a process holds the capability, it can use it; if it doesn't, the action is impossible. This allows for delegation (passing a key to a child process) and fine-grained confinement (giving a process only the specific keys it needs) without constantly querying a central authority.
The commonly-used examples of  capabilities are house keys and car keys.  The car doesn't demand that the driver prove their identity; instead, the drive's posession of the key is sufficient provof that the driver hass the authority to drive the car.
In Lind-Wasm, the 3i system implements this capability model. Instead of checking a user ID, the system enforces security via the syscall table. A Cage possesses the "capability" to invoke a system call only if that specific handler exists in its table. By controlling which handlers are placed in a Cage's table, Grates can precisely define the capabilities available to the application.
## The 3i System: The Capability Bus
The core of this model is the 3i (Intra-process Inter-cage Interface) system.
- The Mechanism: 3i provides a "syscall table with customized jump endpoints for each cage/grate".
- Capability Equivalent: This syscall table acts as the cage's Capability List. A cage cannot invoke a system function unless it has been explicitly granted a handler in its table.
- Efficiency: 3i converts system calls and RPC calls into userspace function calls, avoiding the overhead and security risks of frequent kernel interactions.

## Grates: The Policy Enforcers
A **Grate** acts as a capability wrapper or proxy. It encapsulates external functionality and can interpose on system calls for one or more cages.
### 1. Interposition and Filtering
Grates can "change, filter, block, etc. system calls". This allows for:
- Fine-Grained Control: Monitoring calls externally to the kernel without exposing unnecessary information.
- Functionality Extension: Adding features (like new file systems or tracing) without modifying the microvisor's Trusted Computing Base (TCB).
### 2. Inheritance Hierarchy
Grates operate on an inheritance model similar to capability delegation:
- Delegation: A child cage inherits system calls from its parent upon forking.
- Stacking: If a grate interposes on another grate, it inherits the parent's behavior changes. This allows grates to place restrictions on all descendant cages.

## Lifecycle and Safety
The system distinguishes between forcing an exit and handling the cleanup, mirroring capability revocation and notification:

- `trigger_harsh_cage_exit` (Non-Interposable): This call is triggered by the infrastructure to force a cage to exit. It cannot be intercepted by a grate, acting as an absolute revocation of execution rights.

- `harsh_cage_exit` (Interposable): This acts as a notification that a descendant has exited uncleanly. It is interposable, allowing grates to perform necessary cleanup (like updating the 3i data structure) in response to the failure.

## Example: The "Ransomware Aquarium" (Virtualization without Overhead)
To understand the power of Grates compared to traditional ACLs (Access Control Lists), consider the scenario of executing a suspicious legacy binary—perhaps a game mod or an old invoice parser—that behaves like ransomware.

### The Problem with ACLs
In a traditional system, if the user  suspects a program is malicious, they  might deny it write access to their  files (ACL: `chmod -w`).

**The Result**: The program attempts to write, receives an `EACCES` (Permission Denied) error from the kernel, and likely crashes or detects it is being sandboxed. 

### The Grate Solution: A Reality Distortion Field
In Lind-Wasm, we can wrap the suspicious program in a "Shadow Copy" Grate. Because the Grate interposes on system calls via the 3i interface, it can virtualize the application's environment.

- **The Setup**: The ransomware runs inside a Cage, with a Grate interposed above it.

- **The Bait (READ)**: The ransomware calls `open()` and `read()` on `critical_data.txt`. The Grate intercepts the call, deems it safe, and passes it up to the file system. The ransomware gets the real data.

- **The Trap (WRITE)**: The ransomware attempts to encrypt the data and calls `write()` to overwrite the file.

- **The Interception**: The Grate catches the write call.

- **The Virtualization**: Instead of passing the call to the actual file system, the Grate writes the encrypted data to a temporary, throwaway memory buffer.

- **The Deception**: The Grate returns SUCCESS (e.g., "wrote 1024 bytes") to the ransomware.

- **The Outcome**: The ransomware believes it has successfully encrypted your files and displays its ransom note. In reality, your files on disk are untouched. You can simply terminate the Cage.

### Why This Matters
This demonstrates that a Grate is not just a security gatekeeper; it is a programmable environment.

- **Custom VM Behavior**: The Grate creates a custom "Virtual Machine" where files behave exactly how we want them to (e.g., "writes disappear into the void"), but it does so without the heavy overhead of a traditional VM.

- **Zero-Overhead**: Because 3i converts system calls into simple userspace function calls, this sophisticated interception happens with function-call speed, rather than the heavy context switching of a kernel trap.
- **Flexible Environment**: Since a Lind Grate works by syscall interposition, it can operate in any environment.  A container system like Docker, for example, is fundamentally a namespace within a host OS, and require a kernel with the appropriate mechanisms to support containerization.  Docker, for example, runs only on Linux and OS/X systems; Windows requires installation of a Linux VM (WSL), which actually runs the containers.  In contrast, since Lind isolates applications within a single process, it can run anywhere.
