# Per-Cage Shared Library Loading and Cross-Cage Library Interposition in Lind

## 1. Goal

The next step is to evolve Lind's current remote library-call interposition prototype into a more Lind-native library compartmentalization system.

Instead of having one cage load all shared libraries, Lind should support **per-cage shared-library loading**:

```text
Cage A: main application cage
  loads: application, libc, libpython
  does not load: libz

Cage B: zlib service cage
  loads: libz
  exports: selected zlib symbols
```

When code in Cage A calls a function from a library that is hosted in Cage B, Lind should transparently route the call from Cage A to Cage B.

The high-level idea is:

```text
application cage
  -> dynamic linker / GOT / portal stub
  -> Lind cross-cage dispatcher
  -> library-host cage
  -> target shared library function
  -> result copied back
```

This turns dynamically linked library calls into **policy-controlled cross-cage portals**.

---

## 2. Motivation

The current prototype already demonstrates transparent remote library interposition. For example, Python can run normally without a config file, but when a config file is provided, selected Python `zlib` calls are routed to a remote server.

That is useful, but the remote server is mostly outside Lind. The next step is to bring this idea into Lind's core architecture:

```text
Current model:
  Python cage
    -> Lind interposition
    -> TCP / Unix socket
    -> external remote server
    -> dlopen(libz)

Next model:
  Python cage
    -> Lind interposition
    -> cross-cage call
    -> zlib cage
    -> local dlopen(libz)
```

This is a stronger research direction because Lind controls both sides:

```text
source cage memory
target cage memory
dynamic loading
symbol resolution
routing policy
argument marshalling
copy-in / copy-out
failure handling
security isolation
```

This makes the system look less like an RPC wrapper and more like a **library compartmentalization and placement mechanism**.

---

## 3. Research Framing

A possible research framing:

> Lind supports per-cage dynamic library namespaces and cross-cage library portals, allowing unmodified applications to transparently execute selected dynamically linked library functions in isolated cages according to runtime policy.

This gives Lind a clear role:

```text
Traditional process:
  application + all libraries share one address space

Lind with per-cage library loading:
  application and libraries can be split across isolated cages
  library calls can transparently cross cage boundaries
  placement is controlled by external config
```

This builds toward a paper contribution such as:

```text
1. Per-cage dynamic library namespaces in Lind.
2. Loader/runtime-level interposition of dynamically linked symbols.
3. Cross-cage library-call portals.
4. ABI-aware marshalling for scalar and pointer arguments.
5. Transparent execution of real application/library stacks, e.g. Python -> zlib.
6. Evaluation of local, cross-cage, cross-process, and remote library placement.
```

---

## 4. Target Architecture

### 4.1 Components

```text
+--------------------------+
| Cage A: Application Cage |
|--------------------------|
| Python / main app        |
| libc / libpython         |
| GOT entry for zlib       |
| portal stub              |
+------------+-------------+
             |
             | cross-cage library call
             v
+------------+-------------+
| Lind Runtime Dispatcher  |
|--------------------------|
| policy lookup            |
| argument marshalling     |
| target cage lookup       |
| response handling        |
+------------+-------------+
             |
             v
+------------+-------------+
| Cage B: Library Cage     |
|--------------------------|
| libz.so                  |
| exported zlib functions  |
| local function execution |
+--------------------------+
```

### 4.2 Example Execution

For a Python `zlib.crc32` call:

```text
1. Python code calls zlib.crc32 normally.
2. CPython's zlib module calls the underlying zlib symbol.
3. The symbol/GOT entry in the Python cage points to a Lind portal stub.
4. The portal stub packages the call ID and arguments.
5. Lind routes the request to the zlib cage.
6. The zlib cage invokes its local `crc32` implementation from libz.so.
7. Lind returns the result to the Python cage.
8. Python continues normally.
```

---

## 5. Core Design

## 5.1 Per-Cage Dynamic Loader State

Each cage should have its own dynamic loading namespace.

Instead of a single global view like:

```text
global loaded libraries
global symbol table
global GOT resolution
```

Lind should support:

```text
per-cage loaded library list
per-cage symbol table
per-cage GOT entries
per-cage relocation state
per-cage library search path
per-cage interposition policy
```

This allows the same symbol to resolve differently in different cages:

```text
Cage A:
  crc32 -> portal to Cage B

Cage B:
  crc32 -> local libz.so implementation
```

### Implementation Considerations

Potential internal structures:

```rust
struct CageLoaderState {
    cage_id: CageId,
    loaded_libraries: HashMap<LibraryId, LoadedLibrary>,
    symbol_table: HashMap<SymbolName, SymbolResolution>,
    got_entries: HashMap<SymbolName, GotEntry>,
    interposition_policy: InterpositionPolicy,
}
```

Possible symbol resolution enum:

```rust
enum SymbolResolution {
    Local {
        library: LibraryId,
        address: u64,
    },
    Portal {
        target_cage: CageId,
        library: LibraryId,
        symbol: String,
        call_id: u64,
    },
    Unresolved,
}
```

---

## 5.2 Library-Host Cages

A library-host cage is a cage whose main purpose is to host one or more shared libraries.

Example:

```text
zlib-cage:
  loads:
    libz.so
  exports:
    crc32
    adler32
    compress
    compress2
    uncompress
```

The library-host cage should support:

```text
dlopen of assigned libraries
symbol lookup
local invocation of exported functions
receiving cross-cage call requests
returning results
```

The first prototype can use one library-host cage for `libz.so`.

Later, we can generalize to:

```text
one cage per library
one cage per trust domain
one cage per group of related libraries
one cage per remote placement endpoint
```

---

## 5.3 Cross-Cage Library Portals

A portal is a placeholder function entry in one cage that represents a function physically executed in another cage.

Conceptually:

```text
local call site -> portal stub -> target cage -> real function
```

Portal metadata includes:

```text
source cage
target cage
library name
symbol name
call ID
return type
argument metadata
marshalling policy
```

Possible portal descriptor:

```rust
struct LibraryPortal {
    source_cage: CageId,
    target_cage: CageId,
    library: String,
    symbol: String,
    call_id: u64,
    ret_type: AbiType,
    args: Vec<ArgSpec>,
}
```

---

## 5.4 Argument Marshalling

The existing remote-call marshalling work can be reused for cross-cage calls.

Supported initially:

```text
scalar arguments
input pointers
output pointers
input-output pointers
explicit-size buffers
null-terminated strings, optionally
```

Example metadata:

```json
{
  "symbol": "compress2",
  "call_id": 1,
  "ret": "int",
  "args": [
    {
      "name": "dest",
      "type": "ptr",
      "direction": "out",
      "size_from": "destLen"
    },
    {
      "name": "destLen",
      "type": "ptr",
      "direction": "inout",
      "pointee": "size_t"
    },
    {
      "name": "source",
      "type": "ptr",
      "direction": "in",
      "size_from": "sourceLen"
    },
    {
      "name": "sourceLen",
      "type": "size_t"
    },
    {
      "name": "level",
      "type": "int"
    }
  ]
}
```

Cross-cage marshalling flow:

```text
1. Read scalar arguments from source cage.
2. Copy input buffers from source cage memory.
3. Allocate temporary buffers in target cage.
4. Write copied data into target cage memory.
5. Invoke target function.
6. Copy output/inout buffers back to source cage.
7. Return scalar result.
```

---

## 5.5 Placement Policy

The config should decide which cage owns which library or symbol.

Initial version: whole-library placement.

```json
{
  "cages": [
    {
      "name": "python-main",
      "loads": [
        "libpython.so",
        "libc.so"
      ],
      "imports": [
        {
          "library": "libz.so",
          "from": "zlib-cage"
        }
      ]
    },
    {
      "name": "zlib-cage",
      "loads": [
        "libz.so"
      ],
      "exports": [
        "crc32",
        "adler32",
        "compress2",
        "uncompress"
      ]
    }
  ]
}
```

Later version: function-level placement.

```json
{
  "library": "libz.so",
  "default_placement": "zlib-cage",
  "overrides": {
    "crc32": "local",
    "compress2": "zlib-cage",
    "uncompress": "zlib-cage"
  }
}
```

Recommended implementation strategy:

```text
1. Start with whole-library placement.
2. Add symbol-level overrides later.
3. Keep the config schema flexible enough for future symbol-level policy.
```

---

## 6. Implementation Plan

## Milestone 1: Per-Cage Loader State

Goal:

```text
Make dynamic loading state cage-local instead of globally shared.
```

Tasks:

```text
1. Identify all current global dynamic-loader state.
2. Move loaded library lists into per-cage state.
3. Move symbol tables into per-cage state.
4. Ensure `dlopen` updates only the current cage's loader state.
5. Ensure `dlsym` resolves according to the current cage's namespace.
6. Add debugging output to print each cage's loaded libraries and symbol resolutions.
```

Success criteria:

```text
Cage A can load libX.so.
Cage B can load libY.so.
Cage A does not automatically see symbols from Cage B.
The same symbol name can resolve differently in two cages.
```

---

## Milestone 2: Library-Host Cage

Goal:

```text
Create a cage that loads a target library and exposes selected functions for cross-cage calls.
```

Initial target:

```text
libz.so
```

Tasks:

```text
1. Start a dedicated zlib cage.
2. Load libz.so inside the zlib cage.
3. Resolve selected zlib symbols locally in the zlib cage.
4. Assign call IDs to exported zlib functions.
5. Add a request handler loop or dispatcher inside the zlib cage.
```

Success criteria:

```text
The zlib cage can execute selected zlib functions locally.
The application cage does not need to load libz.so directly for those functions.
```

---

## Milestone 3: Portal Symbol Resolution

Goal:

```text
When a cage imports a symbol hosted by another cage, resolve it to a portal stub instead of a local address.
```

Tasks:

```text
1. Extend symbol resolution to include `Portal`.
2. Generate or register portal stubs for imported remote/cross-cage symbols.
3. Update GOT/table entries to point to the portal stubs.
4. Ensure calls to portal symbols enter the Lind cross-cage dispatcher.
```

Success criteria:

```text
A call from the application cage to a configured zlib symbol enters the portal dispatcher.
The application code does not need to change.
```

---

## Milestone 4: Cross-Cage Call Dispatch

Goal:

```text
Route portal calls from the source cage to the target library-host cage.
```

Tasks:

```text
1. Define cross-cage call request format.
2. Reuse existing scalar/pointer wire format if possible.
3. Add target cage lookup by call ID or portal descriptor.
4. Send request to target cage.
5. Invoke target function.
6. Send response back.
```

Success criteria:

```text
A simple scalar zlib function or toy function can be executed in another cage.
Return values are correctly delivered back to the source cage.
```

---

## Milestone 5: Pointer Copy-In / Copy-Out

Goal:

```text
Support pointer arguments for cross-cage library calls.
```

Tasks:

```text
1. Reuse the existing pointer metadata schema.
2. Copy input buffers from source cage memory to target cage memory.
3. Allocate temporary target buffers.
4. Execute target function.
5. Copy output/inout buffers back to source cage memory.
6. Validate buffer sizes and null pointer behavior.
```

Success criteria:

```text
zlib buffer-oriented functions can run in the zlib cage and return correct results to the Python/application cage.
```

Candidate functions:

```text
crc32
adler32
compress
compress2
uncompress
```

---

## Milestone 6: Python zlib Cross-Cage Demo

Goal:

```text
Replace the current external remote-server Python zlib demo with a Lind-native cross-cage demo.
```

Expected behavior:

```text
without config:
  Python runs normally.
  zlib calls execute locally.

with config:
  Python runs normally.
  selected zlib calls execute in zlib-cage.
```

Tasks:

```text
1. Use existing Python zlib demo as baseline.
2. Add config for zlib-cage placement.
3. Ensure CPython does not need source changes.
4. Route selected zlib calls through cross-cage portals.
5. Compare outputs with local baseline.
```

Success criteria:

```text
Same Python script.
Same Python binary.
No Python source changes.
No zlib source changes.
Config controls whether selected zlib calls run locally or in zlib-cage.
```

---

## Milestone 7: Performance Evaluation

Goal:

```text
Measure overhead and understand when cross-cage library placement is practical.
```

Compare:

```text
local direct call
local interposed call
cross-cage call
cross-process call
remote TCP call
```

Measure function types:

```text
scalar-only calls
small-buffer calls
large-buffer calls
inout-buffer calls
high-frequency cheap calls
low-frequency expensive calls
```

Metrics:

```text
latency per call
throughput
copy overhead
serialization overhead
memory allocation overhead
application-level slowdown
```

---

## Milestone 8: Security / Isolation Evaluation

Goal:

```text
Show that per-cage library loading improves isolation or policy enforcement.
```

Possible demonstrations:

```text
1. Isolate zlib/libpng/libjpeg parser code away from the main application.
2. Restrict the library-host cage's accessible syscalls/resources.
3. Show that a crash in the library cage does not directly corrupt the application cage.
4. Demonstrate policy-based denial or rerouting of selected library functions.
```

Potential future target libraries:

```text
zlib
libpng
libjpeg
OpenSSL / libcrypto
SQLite
```

---

## 7. Key Design Questions

## 7.1 Whole-Library vs Symbol-Level Placement

Whole-library placement:

```text
all libz.so calls go to zlib-cage
```

Pros:

```text
simpler config
simpler reasoning
natural compartment boundary
```

Cons:

```text
less flexible
may route cheap functions unnecessarily
```

Symbol-level placement:

```text
crc32 local
compress2 cross-cage
uncompress cross-cage
```

Pros:

```text
more flexible
better performance tuning
```

Cons:

```text
more complex symbol resolution
harder consistency reasoning
```

Recommended path:

```text
implement whole-library placement first
design config to allow symbol-level overrides later
```

---

## 7.2 Stateful Library Objects

Some libraries return opaque pointers or maintain hidden internal state.

Examples:

```text
sqlite3 *
z_stream *
SSL *
FILE *
```

Possible approach:

```text
Represent target-side objects as handles in the source cage.
```

Example:

```text
sqlite3_open(...) -> handle 17
sqlite3_prepare_v2(handle 17, ...) -> statement handle 93
sqlite3_step(handle 93) -> result
sqlite3_finalize(handle 93)
```

This is probably not needed for the first zlib demo, but it is important for future work.

---

## 7.3 Global State and TLS

Libraries may use:

```text
errno
thread-local storage
global variables
malloc heap state
locale
random number generator state
internal caches
```

Questions:

```text
Should errno be copied back?
Should TLS be per source cage, per target cage, or explicitly virtualized?
Can global state live entirely in the library-host cage?
How should state be reset after failure?
```

Initial recommendation:

```text
Start with mostly stateless/buffer-oriented libraries such as zlib.
Document stateful library handling as future work or later milestone.
```

---

## 7.4 Failure Semantics

Questions:

```text
What happens if the target cage crashes?
What error is returned to the source cage?
Can the target cage be restarted?
Are calls retried?
Are calls at-most-once or exactly-once?
```

Initial recommendation:

```text
For the prototype, use fail-stop semantics:
  if the target cage fails, the portal call returns an error/trap.

Later:
  add restart and retry policies for idempotent functions.
```

---

## 7.5 Memory Sharing vs Copying

Initial approach:

```text
copy-in / copy-out
```

Pros:

```text
simple
safe
preserves isolation
compatible with existing remote-call design
```

Cons:

```text
copy overhead
hard for large buffers
hard for pointer-rich data structures
```

Future optimizations:

```text
shared memory between selected cages
zero-copy buffers
copy-on-write mappings
capability-protected shared regions
```

---

## 8. Immediate Next Steps

Recommended short-term plan:

```text
1. Audit current dynamic-loader state.
2. Identify which structures are global and need to become per-cage.
3. Implement per-cage loaded-library tables.
4. Implement per-cage symbol resolution.
5. Create a simple zlib-host cage.
6. Route one simple scalar or buffer function from application cage to zlib cage.
7. Port the current Python zlib remote demo to use cross-cage execution.
8. Benchmark local vs cross-cage vs remote execution.
```

The first concrete demo should be:

```text
Python script uses zlib normally.

Case 1:
  no config
  zlib executes locally

Case 2:
  config specifies libz.so hosted by zlib-cage
  selected zlib calls execute in zlib-cage

Expected result:
  same Python output in both cases
```

---

## 9. Risks and Challenges

```text
1. Dynamic linker state may be deeply assumed to be global.
2. GOT/table updates may need to become cage-specific.
3. Symbol resolution order may become complicated.
4. Direct calls and indirect calls may require different portal handling.
5. Pointer-rich APIs may be difficult to support generally.
6. Stateful libraries may require object handles.
7. Cross-cage call overhead may be high for tiny functions.
8. Failure semantics need careful definition.
9. The security story requires more than just functional correctness.
```

---

## 10. Paper Direction

If this works, the paper can be framed around:

```text
Per-cage dynamic library namespaces and cross-cage library portals for transparent library compartmentalization in Lind.
```

Possible title directions:

```text
Library Portals: Transparent Cross-Cage Library Interposition in Lind

Per-Cage Dynamic Linking for Library Compartmentalization

Policy-Controlled Library Placement in a WebAssembly Library OS
```

Potential contribution statement:

```text
This paper presents a Lind mechanism that turns dynamically linked library functions
into policy-controlled execution portals. Unlike source-level library sandboxing or
compiler-driven program partitioning, Lind performs loader/runtime-level interposition
inside a sandboxed libOS, allowing selected library calls of unmodified applications to
execute locally, in another cage, in another process, or on a remote host according to
external configuration.
```
