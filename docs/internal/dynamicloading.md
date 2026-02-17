# Dynamic Loading in wasmtime

## Motivation
Dynamic loading reduces the memory footprint by allowing libraries to be loaded only when needed, rather than linking them statically at compile time. It also eliminates the need to recompile the application when a dependent library changes, provided the interface remains compatible. Additonally, in the applications we test, libraries are loaded at runtime using `dlopen()` and `dlsym()`. Therefore, to correctly support these applications, dynamic loading functionality is required.

## Design Decisions
### How Dynamic Loading works in Linux

When a program is executed on Linux, the kernel creates a new process image using execve() and maps the ELF executable into the process’s virtual address space. For statically linked binaries, the kernel sets up the stack and auxiliary data structures and transfers control directly to the program’s entry point. For dynamically linked binaries, the ELF header contains a PT_INTERP segment specifying the dynamic loader (typically `/lib64/ld-linux-x86-64.so.2`). The kernel maps this loader into the same process, transfers control to it, and the loader then loads required shared libraries, resolves symbols, performs relocations, and finally jumps to the program entry point. Crucially, the dynamic loader and the main executable share the same virtual address space and execute within the same process; the loader is not a separate process.

In contrast, WebAssembly (WASM) binaries are not executed directly by the operating system. They run inside a runtime such as Wasmtime, which parses and validates the module, JIT-compiles the code, and instantiates the module. Instantiation involves allocating linear memory—a contiguous, sandboxed memory region —initializing globals and tables, and copying data segments into memory. Unlike ELF binaries, WASM modules do not rely on OS-level virtual memory mapping for code or libraries. Instead, execution and memory management are handled entirely within the runtime, which enforces isolation, bounds checking, and memory safety. 

In the Lind system, dynamic loading support is implemented by extending Wasmtime’s parsing and instantiation mechanisms. Calls such as `dlopen`, `dlsym`, and `dlclose` from glibc are redirected to runtime-provided implementations. The runtime then loads additional WASM modules, allocates memory, resolves symbols, and performs relocation handling—all within the sandboxed environment. Integrating the dynamic loader inside the runtime is necessary because WebAssembly linking requires direct, synchronous modification of internal runtime state, such as function tables and memory bounds, which an external process cannot access without prohibitive serialization overhead. Keeping the dynamic loader internal also avoids the latency of inter-process communication, ensuring that module instantiation remains fast, secure, and fully within the trusted computing base.

## Current Status
Have implemented dynamic loading support in Wasmtime. For applications that are compiled as dynamically linked executables or shared libraries, we are able to support both capabilities below:
1. Launch the application while injecting all required dependent libraries using the `--preload` option (similar to `LD_PRELOAD`).
2. Ensure that `dlopen()`, `dlsym()`, and `dlclose()` are properly resolved at runtime and corresponding libraries loaded.

	  
## Additional Features to be added:
Support for fork, threads and signals within the shared libraries have to be added.
	  
## Changes made to implement dynamic loading:

"To execute WebAssembly applications within Lind, Wasmtime is modified to interface with RawPOSIX for handling system calls such as `mmap`. Specifically, the following changes were implemented in Wasmtime to support dynamic loading:"

### Parsing the dynamic section 
The `dylink.0` custom section within WASM shared libraries is parsed to retrieve dynamic linking metadata. The `load_module` function is responsible for parsing the entire WASM binary to extract all section contents, including code, data, imports, exports, and the dynamic linking information.

### Instantiate the dynamic libraries which are passed using `--preload`
1. Allocate memory for the shared libraries by invoking mmap within rawposix
2. Relocations are applied by invoking `__wasm_apply_data_relocs` and `__wasm_apply_tls_relocs` which are functions within the wasm binary added in the compilation/linking phase.
3. Global Offset Table (GOT) is created and stored in wasmtime. Implemented using a Hash Table which maps symbols to their addresses.
4. Once all modules are loaded into memory, resolve the final address for all functions (GOT.func) and data (GOT.mem) within the GOT table. This is done by the Linker which is part of wasmtime.
### Handling dynamic libraries which are loaded via dlopen()
1. When dlopen(), dlsym() and dlclose() with glibc invokes their respective implementations within Lind.
2. when lind dlopen is invoked, it does the following:
	- Gets the full path of the library by prepending LIND_ROOT to the library name
	- Loads the module/library which involves parsing the wasm file and extracting its section contents including code, data, imports, exports etc.
	- Appends the table with the table of main_module. table refers to indirect function call table. 
	- Instantiate the library which involves
	- Allocate memory for the shared library by invoking mmap within rawposix
	- Relocations are applied by invoking `__wasm_apply_data_relocs` and `__wasm_apply_tls_relocs` which are functions within the wasm binary added in the compilation/linking phase.
	- Resolve the final address for all functions (GOT.func) and data (GOT.mem) within the GOT table. 

3. When dlsym() is invoked, correspond lind function, fetches the address of the function passed as argument, and invokes it.

### Linear Memory Changes
For statically linked binary which has fixed addresses, the memory layout is fixed. Stack comes first, followed by data and heap. In dynamically linked binary, since the code is compiled as position-independent, the memory layout can be determined at runtime. The memory layout for current implementation is as follows:
![Memory Layout (linear memory) in case of wasm binaries with dynamic loading](images/linear_memory.png)


# Generating a WASM Binary for C/C++ applications (Static build)
Let us first explore a WASM binary and steps required to create a WASM binary (build) and then run it.

Programs written in higher level programming languages like C/C++/Rust can be compiled to WASM binaries. A .wasm file is a binary encoding of instructions for a virtual stack-based machine. It must be JIT compiled or AOT compiled or interpreted.

A C/C++ program can be compiled using clang with a WebAssembly target to produce a WASM binary.

```
clang --target=wasm32-unknown-wasi --sysroot=$SYSROOT_FOLDER -O2 gcd.c -o gcd.wasm
```
1. Preprocessing - The C preprocessor handled macros and header expansion
2. Compilation (Frontend) - Clang parses C into LLVM IR
3. Code Generation (LLVM Backend) - Converts LLVM IR into Webassembly instructions as Webassembly object file
4. Linking using `wasm-ld` :
	The object file contains WebAssembly code with unresolved symbols. During linking, `wasm-ld` combines the object file with startup files and static libraries, resolving undefined symbols by pulling in only the required object files from those archives within the `sysroot/` directory. The linker then merges all code, data, memory definitions, and remaining imports into a single final .wasm file

## WASM Binary contents
A `.wasm` binary contains [1] [2] [3]: 
- Magic Header - Identifies the file as a WebAssembly binary
- Version number - Specifies the WebAssembly binary format version
- Sections (Each section contains a section ID, section size, section contents)
	- Type - Defines all functional signatures (parameter and return types) used in the module
	- Import - Declares functions (incl module name), memories, tables or globals that must be provided by the host
	- Function - Lists the type indices of functions defined inside the module
	- Memory - Declares linear memory regions (lower limit and upper limit(optional) required for running the module)
	- Global - Stores internal (non-imported) global variable information including type, whether it is read-only, initialization bytecode
	- Table - Declares indirect function call table
	- Export - Specifies which functions, memories, tables or globals are visible to the host
	- Start - Identifies a function that is automatically executed upon instantiation
	- Element - Initializes table entries
	- Code - Contains the  local variables info and bytecode of internal functions (WebAssembly instructions)
	- Data - Contains memory initialization information. Each entry includes memory index, the starting position (bytecode and much be a constant expression) and initial data

WebAssembly by default just produces a single data section. But when using clang/LLVM to compile C/C++ to wasm, it allocate separate sections for different types of data to support programs written in these languages.
- data (Initialized data)
- tdata (Thread related data)
- .rodata (Read-only data)


## Building a shared WASM library 

A WebAssembly dynamic/shared library is a WebAssembly binary with a special custom section that indicates this is a dynamic library and contains additional information needed by the loader. 

During **compilation**, `-fPIC` flag produces position-independent code whose final address is known only at runtime. Hence, all symbols that is not guaranteed to be local to this shared object will be accessed via `GOT.mem` and `GOT.func` in the generated object code.

During **linking**, `-shared` produces a shared library and `-pie` produces a dynamically linked executable.

To generate **a shared WASM library,** the following compiler and linker flags should be used.
```
CFLAGS=-fPIC
LDFLAGS=-Wl,-shared 
```
Additional linker flags that can be used
```
.   -Wl,--import-memory \
    -Wl,--shared-memory \
    -Wl,--export-dynamic \
    -Wl,--experimental-pic \
    -Wl,--unresolved-symbols=import-dynamic \
```

## Metadata added by linker to the shared WASM binary
- The generated shared WASM library binary will have a custom section called **`dynlink.0`** with the following information:
1. WASM_DYNLINK_MEM_INFO which specifies the memory and table space requirements of the module (memory size, memory alignment, table size, table alignment)
2. WASM_DYLINK_NEEDED - Specifies external modules that this library depends on
3. WASM_DYLINK_EXPORT_INFO - Specify additional metadata about exports
4. WASM_DYLINK_IMPORT_INFO - Specify additional metadata about imports
5. WASM_DYLINK_RUNTIME_PATH = Specify the runtime path, corresponding to DT_RUNPATH in ELF binaries

 - `wasm-ld` will add `__wasm_apply_data_relocs` and `__wasm_apply_tls_relocs` functions to the final WASM binary  which contain hardcoded information about relocation. 

# Running the .wasm file
This .wasm file has to be interpreted and converted to native code and then run within a VM.

The steps involved are :
1. Read the .wasm file
2. Parse the structured sections (types, imports, functions, memory etc) of .wasm binary file
3. Validate the module - The module is validated for type safety, structural correctness, and sandbox rules
4. Compile to native code - Wasmtime translates WebAssembly bytecode into native machine code
5. Create a Store and Link Imports - A store is created to hold runtime state, and all declared imports (eg : WASI functions) are resolved to host implementations
6. Instantiate the Module - Transform the static module into a live execution instance. This involves **allocating** all of the module's Linear Memory, tables, globals in the store, creating separate runtime representations for each function, and **linking them to their resolved imports**. and function instances are allocated memory and initialized inside a new instance
7. Initialize Memory and Tables -After allocation, Wasmtime sets up the module’s initial state by populating memory and tables. Data segments from the Data Section of the module are copied into the linear memory at the offsets specified by the module, often including static strings, arrays, or other constants. Similarly, element segments from the Element Section are used to initialize tables, typically mapping function references for indirect calls.
8. Run the start function


# References
[1]: https://webassembly.github.io/spec/core/binary/modules.html
[2]:https://www.w3.org/TR/wasm-core-1/
[3]: https://coinexsmartchain.medium.com/wasm-introduction-part-1-binary-format-57895d851580
[4]:https://github.com/WebAssembly/tool-conventions/blob/main/DynamicLinking.md
[5]:https://github.com/WebAssembly/tool-conventions/blob/main/Linking.md
[6]:https://emscripten.org/docs/compiling/Dynamic-Linking.html
[7]:https://github.com/bytecodealliance/wasm-tools/blob/main/crates/wit-component/src/linking.rs
[8]:https://www.usenix.org/system/files/sec20-lehmann.pdf



