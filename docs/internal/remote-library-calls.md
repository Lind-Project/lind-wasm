# Remote Library Calls

Inter-process and inter-machine library interposition for Lind cages.

This system allows library functions called inside a Lind cage to be transparently
redirected to a remote server process — on the same machine via Unix domain sockets
or across machines via TCP — without modifying the guest program. The cage continues
to call the function by name as usual; the routing layer intercepts the call at link
time and replaces it with an RPC wrapper.

This mechanism is scoped to **inter-process and inter-machine** use. For
**inter-cage** library interposition (redirecting calls to another cage in the same
Wasmtime process), see the separate inter-cage interposition design.

---

## Components

```
┌─────────────────────────────────────────────────────────┐
│ Lind cage (WASM)                                        │
│   calls library function (e.g. strcpy, rand, add)       │
└────────────────────────┬────────────────────────────────┘
                         │ Wasmtime host function wrapper
                         ▼
┌─────────────────────────────────────────────────────────┐
│ instance_dylink  (linker.rs)                            │
│   get_route(symbol, cage_id) → Remote { endpoint, id } │
│   dispatch_remote_call(endpoint, call_id, args, mem)    │
└────────────────────────┬────────────────────────────────┘
                         │ TCP or Unix socket
                         ▼
┌─────────────────────────────────────────────────────────┐
│ lind-remote-server                                      │
│   reads call_id + args, calls native library function,  │
│   writes result + errno back                            │
└─────────────────────────────────────────────────────────┘
```

| Component | Location |
|---|---|
| Routing config + RPC client | `src/wasmtime/crates/lind-remote-lib/src/lib.rs` |
| Call-time scheduler | `src/wasmtime/crates/lind-remote-lib/src/scheduler.rs` |
| Wasmtime integration | `src/wasmtime/crates/wasmtime/src/runtime/linker.rs` (`instance_dylink`) |
| Server binary | `src/lind-boot/src/bin/lind-remote-server.rs` |
| Examples | `tests/library-interposition-examples/` |

---

## Routing Config

The routing policy is expressed in a JSON file whose path is given by the
`LIND_REMOTE_CONFIG` environment variable. If the variable is unset or the file
cannot be read, every function falls through to local execution.

The config is loaded once at first use and cached in a `OnceLock<RoutingState>`.

### Top-level fields

```json
{
  "default_route": "local",
  "remotes": { ... },
  "routes": [ ... ]
}
```

- `default_route` — fallback decision for symbols not listed in `routes`. Only
  `"local"` is supported today.
- `remotes` — named endpoint definitions reused across multiple route entries.
- `routes` — ordered list of per-symbol routing rules.

### Remote endpoint

```json
"remotes": {
  "my_server": { "endpoint": "unix:///tmp/my.sock" },
  "far_server": { "endpoint": "tcp://192.168.1.10:9000" }
}
```

Supported URI schemes:
- `unix://<absolute-path>` — Unix domain socket (same machine, lower overhead).
- `tcp://<host:port>` — TCP socket (local or remote machine). Nagle is disabled
  (`TCP_NODELAY`) so each request-response round trip is sent immediately.

### Route entry

```json
{
  "symbol":  "strcpy",
  "route":   "remote",
  "remote":  "my_server",
  "call_id": 1,
  "cageid":  2,
  "args": [ ... ]
}
```

| Field | Required | Description |
|---|---|---|
| `symbol` | yes | Exact name of the library function to intercept. |
| `route` | yes | `"remote"` or `"local"`. |
| `remote` | if remote | Key in `remotes` that provides the endpoint. |
| `call_id` | if remote | Numeric identifier sent on the wire so the server can dispatch to the right handler. |
| `cageid` | no | If set, this rule applies only to the cage with this ID. If absent, it applies globally to all cages. Per-cage rules take priority over global ones. |
| `args` | no | Argument metadata for pointer marshaling. Omit for pure-scalar functions. |

### Argument metadata

When a function takes pointer arguments, `args` describes each parameter in
declaration order so the client can read/write the correct bytes from/to WASM
linear memory before and after the RPC.

```json
"args": [
  { "type": "scalar" },
  { "type": "ptr", "direction": "in",  "null_terminated": true },
  { "type": "ptr", "direction": "out", "same_as_arg": 1 },
  { "type": "ptr", "direction": "inout", "size_arg": 3 }
]
```

Each entry is one of:

**Scalar** — passed as a plain `u64`, no memory access needed:

```json
{ "type": "scalar" }
```

**Pointer** — the WASM argument is a linear-memory offset pointing to a buffer:

| Field | Description |
|---|---|
| `direction` | `"in"` (read from guest, send to server), `"out"` (server writes, copy back to guest), `"inout"` (both). |
| `size_arg` | Index of the scalar argument that holds the buffer length. |
| `null_terminated` | Scan WASM memory for `'\0'`; size includes the terminator, capped at 4096 bytes. |
| `same_as_arg` | Use the resolved `alloc_size` of the pointer argument at this index. Used for `out` destinations whose size equals a corresponding `in` source (e.g. `strcpy` dest = strlen(src)+1). |

Exactly one of `size_arg`, `null_terminated`, or `same_as_arg` must be provided
for each `ptr` entry.

---

## Routing Lookup

`get_route(symbol, cage_id)` is called at **link time** (inside `instance_dylink`)
for each exported function of a loaded library module. The resolution order is:

1. If `cage_id` is `Some(id)`, look up the per-cage table for that cage.
2. If no per-cage match, look up the global route table.
3. If no global match, return `Local` (the default decision).

Because the `OnceLock` is initialised once, the routing state is immutable after
the first lookup. The decision made at link time is baked into the closure
installed for that symbol and does not change for the lifetime of the linker.

---

## Wasmtime Integration

The integration point is `instance_dylink` in `linker.rs`. This method is called
whenever a dynamic library is loaded into a cage's `Store`. For each exported
function of the library module it checks the routing decision:

**Remote** — installs a Wasmtime host-function wrapper that:
1. Converts `Val` params to `Vec<u64>` scalar args.
2. Obtains the base pointer of the cage's linear memory via
   `StoreOpaque::all_memories()`.
3. Calls `dispatch_remote_call` which handles pointer marshaling and the wire
   protocol.
4. Writes the returned `u64` back into the `results` slice, cast to the correct
   `ValType`.

The wrapper also consults `Scheduler::decide` at call time (currently always
`Remote`; see [Scheduler](#scheduler) below).

**Local** — links the original WASM function directly via `call_nested` (a
re-entrant variant of Wasmtime's internal `call_unchecked_raw` that defers asyncify
`on_called` processing to the outer loop). This path has zero overhead compared to
a direct wasm-to-wasm call.

```
instance_dylink
  for each exported Func:
    get_route(name, cage_id)
      Remote → host fn wrapper → dispatch_remote_call → RPC
      Local  → call_nested wrapper (asyncify-safe)
```

The `skiplist` parameter lets callers exclude specific symbols from being wrapped
(e.g. `signal_callback` must not be intercepted). Internal dylink symbols and
`__fpcast_emu_*` functions are always skipped regardless.

---

## Wire Protocol

All integers are **little-endian**.

### Scalar request / response

Used when no `args` metadata is present for the function.

```
Request:
  call_id:  u32
  num_args: u32
  arg[0]:   u64
  ...
  arg[N-1]: u64

Response:
  result:   u64
  errno:    i32
```

### Extended request / response (pointer arguments)

Used when `args` metadata declares at least one `ptr` entry.

```
Request:
  call_id:    u32
  num_args:   u32
  arg[0..N]:  u64   (pointer positions zeroed out)
  num_ptrs:   u32
  for each Ptr arg in declaration order:
    alloc_size: u32
    data:       [alloc_size bytes]   (omitted for Out-direction pointers)

Response:
  result:      u64
  errno:       i32
  num_out_bufs: u32
  for each Out/InOut arg in declaration order:
    size: u32
    data: [size bytes]
```

---

## Client-side Dispatch (`dispatch_remote_call`)

`dispatch_remote_call` is the single entry point called by the Wasmtime wrapper:

1. **Look up argument metadata** (`get_meta(symbol)`).
2. **Two-pass size resolution** (when metadata is present):
   - Pass 1: resolve `Arg(j)` sizes (from scalar args) and `NullTerminated` sizes
     (scan linear memory for `'\0'`).
   - Pass 2: resolve `SameAsPtrArg(j)` entries using the sizes computed in pass 1.
3. **Zero out pointer positions** in the scalar args vector — the server does not
   receive raw WASM pointers.
4. **Read In/InOut buffers** from WASM linear memory into `Vec<u8>` payloads.
5. **Send extended RPC** (`rpc_call_with_ptrs`) or **plain RPC** (`rpc_call`) if no
   pointer metadata.
6. **Write Out/InOut results** returned by the server back into WASM linear memory.
7. **Return** the `u64` result.

The caller (`instance_dylink` wrapper) is responsible for obtaining `mem_base` —
the base pointer of the cage's linear memory — and converting the `u64` result to
the appropriate `ValType`.

---

## Scheduler

`Scheduler::decide(symbol)` is a call-time hook consulted by the Wasmtime wrapper
after the link-time routing decision has already determined the function is
*eligible* for remote dispatch. It can override the decision back to `Local` on a
per-call basis (e.g. based on server load or observed latency).

Current implementation is a placeholder that always returns `Remote`. The interface
is in `src/wasmtime/crates/lind-remote-lib/src/scheduler.rs`.

---

## Per-Cage Routing

A route entry with a `cageid` field applies only to the cage that has that ID.
Multiple cages can call the same symbol with different routing policies:

```json
"routes": [
  { "symbol": "strcpy", "route": "local",  "cageid": 1 },
  { "symbol": "strcpy", "route": "remote", "remote": "unix_server", "call_id": 1, "cageid": 2 },
  { "symbol": "strcpy", "route": "remote", "remote": "tcp_server",  "call_id": 1, "cageid": 3 }
]
```

Cage 1 executes `strcpy` locally; cage 2 sends it over a Unix socket; cage 3 sends
it over TCP. See `tests/library-interposition-examples/per-cage-routing/` for a
runnable example.

---

## Examples

All examples live under `tests/library-interposition-examples/`. Each directory
contains a `run.sh` that builds the necessary binaries, starts the server(s), and
runs the cage.

| Directory | What it demonstrates |
|---|---|
| `basic/` | Minimal end-to-end: two scalar functions (`add`, `mul`) redirected to a Unix socket server. |
| `per-cage-routing/` | Three cages, same symbol (`strcpy`), three different routing policies (local / Unix / TCP). |
| `pointer-marshaling/` | `write` and `strcpy` with pointer arguments; shows `size_arg`, `null_terminated`, and `same_as_arg`. |
| `rand-library/` | `rand` delegated to a server that calls the native libc implementation. |
| `zlib-compression/` | `compress` / `uncompress` from zlib redirected to a server with the native zlib installed. |

---

## Adding a New Remotely-Dispatched Function

1. **Add a route entry** to the routing config:

   ```json
   { "symbol": "my_func", "route": "remote", "remote": "srv", "call_id": 42 }
   ```

   For pointer arguments, add an `args` array describing each parameter.

2. **Implement the server handler** for `call_id = 42` in the server binary. The
   server reads args with `read_request` (scalar) or `read_call_id` + `read_scalar_args`
   + `read_ptr_sections` (extended), calls the native function, and writes back with
   `write_response` or `write_response_with_ptrs`.

3. **Start the server** before running the cage. The server config is a separate JSON
   file listing which shared library handles each `call_id`.

No changes to the cage's source code or WASM binary are needed.

---

## Limitations

- **One connection per call**: each `dispatch_remote_call` opens a fresh socket
  connection. Connection pooling is not yet implemented.
- **Scalar results only**: the protocol returns a single `u64` result. Functions that
  return structs or write results exclusively through pointer arguments must encode
  their primary return value as a scalar error code.
- **No errno propagation**: the `errno` field in the wire protocol is received but
  currently not written back into the guest's errno variable.
- **No callbacks**: functions that accept function-pointer arguments cannot be
  dispatched remotely with this protocol.
- **Scheduler is a stub**: `Scheduler::decide` always returns `Remote`. Load-based
  or latency-aware fallback to local execution is not yet implemented.
