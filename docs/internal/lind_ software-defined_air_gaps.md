

# ---

**Software-Defined Air-Gaps: Implementing Capability-Based Security with Lind-Wasm Grates**

## **Abstract**

Traditional secure computing infrastructure relies on physical isolation ("air gaps") or rigid kernel-level virtualization (VMs/containers) to protect high-risk data. This paper introduces a paradigm shift: **Software-Defined Air-Gap Infrastructure** using Lind-Wasm. By leveraging **Grates** (programmable interposition layers) and the **3i** (Intra-process Inter-cage Interface) system, Lind implements a capability-based security model that enforces isolation at the system call boundary. This architecture allows for portable, mathematically isolated execution environments that function independently of the host infrastructure, effectively democratizing high-security computing.

## ---

**1\. Introduction: Beyond Access Control Lists**

Current operating systems predominantly rely on Access Control Lists (ACLs) for security. In an ACL model, the kernel checks an entity's *identity* against a list of permissions (e.g., "User X can read File Y"). This approach is coarse-grained and reactive; if an application is compromised, the kernel must actively deny its requests, often leading to crashes or instability.

Lind-Wasm adopts a **Capability-based** model. In this system, security is based on *possession*, not identity. A process (or "Cage") cannot perform an action unless it holds the specific "token" (capability) to do so. In Lind, these tokens are the system call handlers themselves. If a Cage lacks the handler for socket(), it is not that the firewall blocks the connection—it is that the very concept of networking does not exist for that process.

## ---

**2\. Architecture: Cages, Grates, and the 3i Bus**

The Lind architecture comprises three primary components that enable this capability model:

### **2.1 The Cage (Isolation)**

A **Cage** is a lightweight isolation boundary within a process, utilizing Software Fault Isolation (SFI) to protect memory. It encapsulates the application's memory and bookkeeping, similar to a typical OS process but running entirely in user space.

### **2.2 The 3i System (The Capability Bus)**

The **3i (Intra-process Inter-cage Interface)** is the mechanism that routes calls between cages. It provides a "syscall table with customized jump endpoints for each cage/grate". This table acts as the **Capability List**; a Cage can only execute the functions populated in its specific 3i table. This design converts system calls into fast userspace function calls, avoiding the overhead of kernel context switches.

### **2.3 The Grate (The Policy Enforcer)**

A **Grate** is a specialized Cage that "interposes" on the system calls of descendant cages. It acts as a **Capability Wrapper**.

* **Interposition:** Grates can "change, filter, block, etc. system calls" before they reach the microvisor or the host kernel.  
* **Inheritance:** Grates follow a strict inheritance hierarchy. A child cage inherits the system call handlers of its parent, and stacked grates inherit the behavior changes of the grates above them.

## ---

**3\. Case Study A: The "Ransomware Aquarium"**

*Virtualization without Overhead*

The power of Grates lies in their ability to create a "Reality Distortion Field" for an application. Consider the execution of a suspicious binary (e.g., potential ransomware) that attempts to encrypt user files.

* **Traditional Approach (ACL):** The system denies write access. The malware receives an "Access Denied" error, detects the sandbox, and aborts or crashes.  
* **The Lind Approach (Grate):** We wrap the malware in a **"Shadow Copy" Grate**.  
  1. **Read:** When the malware calls read(), the Grate passes the call through, allowing it to read the real file.  
  2. **Write:** When the malware calls write() to save the encrypted file, the Grate intercepts the call via the 3i interface. Instead of writing to disk, the Grate writes the data to a throwaway memory buffer.  
  3. **Deception:** The Grate returns SUCCESS to the malware. The ransomware believes it has succeeded, while the underlying filesystem remains untouched.

This allows researchers to analyze malware behavior safely by virtualizing its reality at the syscall level, with zero kernel overhead.

## ---

**4\. Case Study B: Software-Defined Air-Gap Infrastructure**

*Democratizing P4 Data Research*

Handling P4 (high-risk) data typically requires physical air-gapped infrastructure (like the UC Berkeley Secure Research Data & Computing Facility (SRDC)). This creates significant friction: researchers must move their code, tools, and data into a restricted, hard-to-maintain physical environment.

Lind enables a **Software-Defined Air-Gap** that is portable and provably secure.

### **4.1 Subtractive Capabilities**

We can construct a "Zero-Trust Grate" that removes network capabilities from the 3i table entirely. The application inside the Cage does not face a blocked port; it faces a universe where the socket, bind, and connect verbs do not exist. Data exfiltration is rendered impossible by the runtime environment.

### **4.2 Selective Isolation**
A Grate can be constructed to forbid the egress of specific objects, or even specific objects on specific interfaces.  For example, local scratch files in memory can be permitted, and transmission of data permitted except for specified, named data objects.  

### **4.3 Content-Aware Exfiltration Control**

Unlike a physical cable cut, a Grate is intelligent. It can interpose on write() calls to standard output or disk. The Grate can scan outgoing buffers for sensitive patterns (e.g., Social Security Numbers) in real-time. If a violation is detected, the Grate can trigger a harsh\_cage\_exit, instantly terminating the session and cleaning up resources.

### **4.4 The "Apptainer" Evolution**

This architecture solves the compliance auditability problem. Instead of auditing a physical server room's configuration, compliance officers can audit the **Grate code** itself. The security policy is baked into the application binary. This allows a "Lind Container" to be deployed on commodity cloud infrastructure (AWS, Google Cloud) while maintaining P4-level isolation.

## ---

**5\. Lind vs. Traditional Containers**

While Docker and LXC provide isolation, they fundamentally rely on the host kernel.

* **Kernel Inception:** Lind utilizes a **Microvisor**, a POSIX-compliant kernel running within a process. This allows Lind to run in environments without a native kernel, such as inside a **web browser** (via Wasmtime).  
* **Safety:** A crash in a Docker container can potentially compromise the host kernel. A crash in a Lind Cage is contained within the user-space process.

## ---

**6\. Conclusion**

Lind-Wasm transforms "Air Gap" from a physical constraint into a software asset. By utilizing Grates to enforce fine-grained, capability-based security, Lind allows for the creation of secure, portable, and programmable execution environments. This enables a new class of computing where the security travels with the data, rather than the data being forced into a secure room.