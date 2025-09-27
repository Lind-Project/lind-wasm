# Memory Management and Vmmap

## What is a Vmmap?

A vmmap is a tool for managing a process’s memory layout within an operating system. It provides detailed insights into allocated memory regions, including the heap, stack, and memory-mapped files. Additionally, it displays access permissions (read-only, read-write, executable), memory region boundaries, sizes, and any mapped files.

## Motivation

Wasmtime traditionally manages memory using WebAssembly’s linear memory model, where each instance gets a contiguous memory block divided into 64 KiB pages. This memory can grow or shrink dynamically within defined constraints. Since Lind emulates processes as cages within a single address space, tracking allocated memory regions per cage is essential.

Attempts to provide POSIX-like interfaces for WASM, such as wasi-libc and emscripten, rely on WASM's memory.grow feature to expand available memory. Both implement custom malloc() functions that use memory.grow to extend the heap while preventing system mmap operations. Alternatively, they simulate file-backed mmap by invoking memory.grow and manually copying file contents into the allocated region.

To address this, we eschew memory.grow and integrate a vmmap system into Lind that more closely resembles POSIX-based memory management. This allows proper implementation of syscalls like brk(), mmap(), and mprotect() for memory allocation, deallocation, and permission management. It also ensures accurate memory region copying when forking cages. Further justification for the need for a vmmap is provided in the later section "Why the Vmmap is Necessary."

## Vmmap Implementation Overview

The vmmap internally uses a [discrete interval tree](https://docs.rs/nodit/latest/nodit/) to manage memory regions efficiently. This data structure functions similarly to a balanced tree, enabling fast lookups, insertions, and deletions of memory mappings. It supports optimized allocation by quickly identifying contiguous free memory blocks. Additionally, it ensures proper handling of updates, such as modifying protections or removing entries, by correctly managing overlapping regions through splitting or merging. Other key features include address range queries to validate memory access and enforce permissions, as well as functions for translating addresses between user space and system memory.

### Why the Vmmap is Necessary

Without a vmmap, syscalls like mmap() and munmap() could still be implemented using a greedy approach with memory.grow, similar to how other systems simulate file-backed mmap, as described above. However, this method would be unsuitable for multi-processing and would violate POSIX compliance, as we explain in this section.

#### fork()

The fork() system call requires duplicating the parent process’s memory space for the child. Properly replicating memory requires tracking protections and distinguishing shared memory regions. Memory regions possess distinct permissions—either defined at creation or modified through mprotect(). Consequently, a simple bulk memory copy cannot accurately preserve these protections without tracking each region individually. Additionally, memory regions mapped with MAP_SHARED must be tracked individually to ensure proper sharing between cages. Without a mechanism like a vmmap, there is no way to distinguish shared regions from non-shared ones. By tracking shared regions, we can then use [mremap](https://man7.org/linux/man-pages/man2/mremap.2.html) when forking to create a shareable mapping between cages.

#### brk()

The brk() system call expands the heap linearly, ensuring contiguous allocation as required by libc and other libraries. Many functions, including malloc(), depend on this guarantee. Without a vmmap, memory allocation could use the aforementioned greedy approach, but this might interleave heap regions with other mappings created by mmap(), violating POSIX compliance and leading to library failures.


### mmap()/munmap()/mprotect()

It's necessary to manage memory allocated or modified using these calls to support the proper functiong of fork() and brk() as mentioned above.

### Additional Benefits:

- Reduced fragmentation: Without memory tracking, greedy allocation wastes space by failing to reuse deallocated pages. This is particularly crucial since cages are limited to 4GB of address space.
- Improved memory safety: Heap overflows are less likely to impact valid mappings, as heaps and other memory regions remain isolated unless explicitly mapped with MAP_FIXED.

# System Calls

The implementation of mmap, brk, and sbrk interacts with vmmap, ensuring efficient allocation, deallocation, and permission enforcement for different types of memory regions.

### mmap()

mmap provides a mechanism for mapping memory regions with specific properties, such as anonymous memory for heap growth or file-backed mappings for shared memory. It allows fine-grained control over memory protection (PROT_READ, PROT_WRITE), allocation strategies (MAP_PRIVATE, MAP_SHARED), and address-space placement (MAP_FIXED).
To ensure that memory mappings remain manageable, mmap works with vmmap to search for available memory regions. When vmmap searches for a free range, it always starts from the bottom of the address space and grows upwards. This minimizes fragmentation and avoids conflicts with the heap, which is placed at the top of memory and grows downwards.

**How It Works**

1. Memory Region Identification:
    - If MAP_FIXED is not set, vmmap searches for a suitable free memory region.
    - If MAP_FIXED is specified, the requested address is used directly.
2. Memory Protection and Flags Enforcement:
    - Only a restricted set of flags are allowed to prevent unintended behavior.
    - Execution permissions (PROT_EXEC) are explicitly disallowed for security reasons.
3. Address Translation and System Invocation:
    - The selected virtual address is translated into a system address.
    -The actual mmap operation is invoked on the host system with MAP_FIXED to ensure deterministic placement.
4. Updating the vmmap:
    - If the mapping is successful, vmmap is updated to reflect the allocated region, including its permissions and backing type (anonymous or file-backed).

### munmap()

munmap is used to release memory mappings previously allocated via mmap. Unlike traditional implementations that return memory to the OS, Lind’s munmap only marks the region as inaccessible by setting it to PROT_NONE, while retaining it within the process's address space.

**How It Works**

1. Address Validation:
    - The target address must be aligned to page boundaries.
    - The region must exist within vmmap and must not contain protected memory.
2. Memory Protection Adjustment:
    - Instead of actually deallocating memory, the affected region is marked as PROT_NONE. The memory remains allocated but becomes inaccessible.
3. Updating vmmap:
    - The mapping entry is removed from vmmap, ensuring that the region is available for future allocations.

### brk()/sbrk()

brk and sbrk provide a mechanism to dynamically expand or shrink the heap by adjusting the program break. This is essential for memory allocation routines such as malloc, which rely on contiguous heap growth.
In Lind, the heap is always placed at the top of the memory space, right after the stack region, and grows downwards. This is opposite to the direction in which mmap allocates memory (bottom-up), ensuring that mmap-allocated regions do not typically interfere with heap growth.


**How It Works**

1. Tracking the Program Break:
    - The program break corresponds to the end of the heap region in vmmap.
    - sbrk(0) returns the current break, while sbrk(N) attempts to increase the heap by N bytes.
2. Heap Expansion:
    - When increasing the program break, the system first verifies that the requested range does not overlap with existing mappings.
    - If space is available, the permissions of the new memory region are updated to match the heap’s permissions.
    - The vmmap entry is updated to reflect the new program break.
3. Heap Shrinking:
    - If the program break is decreased, memory beyond the new limit is marked as inaccessible (PROT_NONE) instead of being deallocated immediately, similar to munmap.
    - The vmmap entry is updated accordingly.
