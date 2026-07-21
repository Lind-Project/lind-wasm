# 03-buffers — design: caller-allocated output buffers (copy-out)

This iteration adds **copy-out** marshalling: functions that write into a buffer
the *caller* allocates (`char *out, size_t n`). It builds on `02-strings`
(copy-**in**) and introduces the problem that the copy-back length is a per-function
**contract**, not something derivable from the C type.

## The core problem: how many bytes were written?

`char *out, size_t n` tells you the *capacity* but not how many bytes the function
actually wrote — that's a runtime fact governed by the function's contract, which
the type system doesn't carry. So it can't be inferred; it must be **declared** per
function in the manifest. Real-world contracts collapse to a small set:

| Contract | Copy back… | manifest |
| --- | --- | --- |
| Return value is the count | `min(ret, cap)` | `outbuf,cap=C,len=ret` |
| NUL-terminated (out is a C string) | up to & incl. the first NUL | `outbuf,cap=C,len=nul` |
| Whole buffer filled | `cap` | `outbuf,cap=C,len=cap` |
| Length written to a `size_t*` out-param | that param's value | `outbuf,cap=C,len=arg<M>` + `outlen` |

This example implements one function per contract (`to_upper`, `greet`,
`fill_pattern`, `extract_word`).

## Flow of a copy-out call (`to_upper("hello world", buf, 64)`)

```
app: to_upper("hello world", buf, 64)   (buf caller-allocated, cap 64)
  │  native call → stub in libbufdemo.so
  ▼
STUB (generated)
  in0  = cstr_bytes("hello world")                 (copy-in source)
  out1 = &mut buf[..64]                             (caller's output buffer view)
  args = [ Arg::Buf(&in0),
           Arg::Out { dst: out1, len: OutLen::Ret },
           Arg::USize(64) ]
  call_buf("to_upper", &mut args)
  ▼
SandboxedLib::call  (lock the global Mutex)
  Phase 1  allocate + copy-in:
             p_in  = guest_malloc(12); write "hello world\0"
             p_out = guest_malloc(64)           (output buffer, not written yet)
           params = [p_in, p_out, 64]
  Phase 2  invoke → guest to_upper writes "HELLO WORLD" into p_out, returns 11
  Phase 3  read out-length params (none here)
  Phase 4  copy-out:  len = min(ret=11, cap=64) = 11
                      read 11 bytes from p_out → copy into buf
  Phase 5  free p_in, p_out
  ▼
STUB: return 11
```

The other three contracts differ only in Phase 4's length:
- `nul` — scan the guest buffer for the first NUL, copy up to & including it.
- `cap` — copy the full capacity.
- `arg<M>` — Phase 3 reads the `size_t*` out-param the guest wrote; Phase 4 uses
  that value (and Phase 3 also writes it back into the caller's `size_t`).

## Changes by file

### A. Marshalling engine — `lind-boot`

**`src/lind-boot/src/lind_wasmtime/sandboxed_lib.rs`**

- **`Arg` gains** `USize(usize)`, `Out { dst: &mut [u8], len: OutLen }`, and
  `OutLen(&mut usize)`. `dst.len()` is the output buffer's capacity.
- **New `OutLen` enum** — `Ret | Nul | Cap | FromArg(usize)`, the four copy-back
  length contracts. `FromArg(i)` indexes the `OutLen` arg at position `i`.
- **`call` reworked** to `&mut [Arg]` with five phases: (1) allocate in-/out-buffers
  and copy inputs in; (2) invoke; (3) read `OutLen` params back (into the caller's
  `size_t` and an internal length table); (4) resolve each `Out` buffer's copy-back
  length and copy it into the caller's slice; (5) free. Returns `0` for `void`.
- **New primitives** — `write_mem`, `read_mem`, `read_u32`, `guest_cstr_len`; `copy_in`
  now builds on `write_mem`. `SIZE_T_BYTES = 4` (wasm32 `size_t`).

**`mod.rs`, `lib.rs`** — also re-export `OutLen`.

### B. Code generation — `lind-sharedlib-poc/tools/gen_stubs.sh`

- New arg specs: `usize`, `outbuf,cap=C,len={ret|nul|cap|arg<M>}`, `outlen`; new
  return type `void`.
- Marshalled stubs are built as an `Arg` array: `cstr` → `Arg::Buf`, `outbuf` →
  `Arg::Out { dst: &mut buf[..cap], len: OutLen::… }` (capacity taken from arg `C`),
  `outlen` → `Arg::OutLen(&mut *p)`. `len=arg<M>` becomes `OutLen::FromArg(M-1)`.
- Imports/helpers (`Arg`, `OutLen`, `cstr_bytes`, `call_buf`) are emitted only when
  needed; the all-`i32` scalar fast path (example 01) is unchanged.

### C. The example — `lind-sharedlib-poc/examples/03-buffers/`

`guest.c` (four functions + `guest_malloc`/`guest_free`, uses libc), `demo.c`,
`functions.txt`, `Makefile` (`LIB := bufdemo`, `PRELOAD` libc/libm), `stub/`
(cdylib → `libbufdemo.so`), and the generated `stub/src/lib.rs`.

## Run

```bash
make run          # wasm-sandboxed
make run-native   # baseline
```

```
to_upper      -> "HELLO WORLD" (11 bytes)
greet         -> "Hello, lind!"
fill_pattern  -> "ABCDEFGHIJ"
extract_word  -> "sandboxed" (len=9)
```

## Still out of scope

In/out buffers (guest also reads the buffer), structs, callbacks, concurrency.
