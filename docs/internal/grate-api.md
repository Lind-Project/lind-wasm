# Grate API Reference

This page documents the C API declared in [`src/glibc/lind_syscall/lind_syscall.h`](https://github.com/Lind-Project/lind-wasm/blob/main/src/glibc/lind_syscall/lind_syscall.h), the public threei / grate interface header. It is installed into the lind-wasm sysroot include path, so grate authors can simply write below includes when they are writing C grates:

```c
#include <lind_syscall.h>
```

The header exposes the minimal set of calls a grate (or any other interposition component) needs to:

- invoke syscalls via `make_threei_call()`,
- register or deregister syscall handlers via `register_handler()`,
- copy data between cages/grates in a controlled way via `copy_data_between_cages()`,
- propagate a handler table to another cage/grate via `copy_handler_table_to_cage()`.

Grates can be written in C or in Rust. The C API below is the canonical interface; Rust grates use the [`grate-rs`](https://github.com/Lind-Project/lind-wasm-example-grates/tree/main/lib/grate-rs) library, which links against the same four sysroot symbols and wraps them in safe Rust APIs — the semantics documented here apply unchanged. See [Writing grates in Rust](#writing-grates-in-rust-grate-rs) below.

For the concepts behind grates and 3i (why interposition works this way, how grates compose, etc.), see [Grates](grates.md) and [Threei](3i.md). This page focuses on how to *use* the API.


## Common conventions

### Cage IDs

Every cage (and grate — a grate is just a cage) has a cage ID, which has the same meaning as pid in Linux. Inside a grate, `getpid()` returns the grate's own cage ID.

### (value, cageid) argument pairs

Cages have separate address spaces and users _typically_ can only see 32-bit guest address (upper 32-bit is hidden to guest by default), so a raw pointer is meaningless without knowing which cage it belongs to. 3i therefore passes every syscall argument as a *(value, cageid)* pair: the value itself, plus the ID of the cage whose address space the value should be interpreted in. For non-pointer arguments (flags, lengths, fds) the cageid is ignored by the eventual handler, but the slot must still be filled.

### `GRATE_MEMORY_FLAG`

```c
#define GRATE_MEMORY_FLAG ((uint64_t)1 << 63)
```

Wasm code addresses its own linear memory with 32-bit offsets, but the host side works with host addresses. When a grate passes a pointer that lives in its **own** linear memory (e.g. a buffer it allocated with `malloc`), it must tell the syscall layer to translate that offset into a host address. Do this by OR-ing `GRATE_MEMORY_FLAG` into the corresponding `argNcageid`:

```c
arg2cageid = my_cageid | GRATE_MEMORY_FLAG
```

Pointers that came *from another cage* (e.g. the intercepted caller's buffer, received through the handler arguments) are already host-translated, so pass them through with their original cageid, **without** the flag.

### Return values and errno

The 3i-level calls on this page (`register_handler`, `copy_data_between_cages`, `copy_handler_table_to_cage`) return raw results
without errno translation: `0` on success, or a 3i error code. The two you will encounter are:

| Constant | Value | Meaning |
|---|---|---|
| `ELINDAPIABORTED` | `0xE0010001` | The call was rejected (invalid arguments, missing handler table, invalid copytype, out-of-bounds address range, ...) |
| `ELINDESRCH` | `0xE0010002` | The source or target cage is exiting and can no longer be operated on |

`make_threei_call` lets you choose the errno behavior explicitly via its last parameter (see below).


## `make_threei_call`

```c
int make_threei_call(unsigned int callnumber,
    uint64_t callname,
    uint64_t self_cageid, uint64_t target_cageid,
    uint64_t arg1, uint64_t arg1cageid,
    uint64_t arg2, uint64_t arg2cageid,
    uint64_t arg3, uint64_t arg3cageid,
    uint64_t arg4, uint64_t arg4cageid,
    uint64_t arg5, uint64_t arg5cageid,
    uint64_t arg6, uint64_t arg6cageid,
    int translate_errno);
```

The canonical entry point for issuing a syscall through 3i. This is what a grate uses to **forward** an intercepted call downward (to the next grate in the stack, or ultimately to RawPOSIX), possibly after inspecting or rewriting its arguments.

**Parameters**

| Parameter | Description |
|---|---|
| `callnumber` | The syscall number to invoke. |
| `callname` | Currently unused by the trampoline; pass `0`. |
| `self_cageid` | The cage making the call — for a grate, its own cage ID (`getpid()`). |
| `target_cageid` | The cage the syscall should *act on*. When forwarding an intercepted call, this is the **originating cage**, so that e.g. a forwarded `fork` duplicates the caller and a forwarded `mmap` maps into the caller's address space — not the grate's. |
| `argN`, `argNcageid` | The six syscall arguments as (value, cageid) pairs. For a pointer into the grate's own memory, set `argNcageid = self_cageid \| GRATE_MEMORY_FLAG`. For arguments received from the intercepted caller, pass the value and cageid through unchanged. Fill unused slots with `0` (or `NOTUSED`). |
| `translate_errno` | `TRANSLATE_ERRNO_ON` (1): return values in `[-255, -1]` are treated as `-errno`; errno is set and `-1` is returned. `TRANSLATE_ERRNO_OFF` (0): the raw return value is passed through untouched. |

**Choosing `translate_errno`:** inside a grate handler you usually want `TRANSLATE_ERRNO_OFF`, because your handler's return value is itself delivered back through the syscall path — the *originating* cage's libc will do the errno translation. Use `TRANSLATE_ERRNO_ON` when the grate calls a syscall for its own purposes and wants normal POSIX `-1`/`errno` semantics.

**Example** — an `open` handler that redirects the path, then forwards the call as the original cage:

```c
int open_grate(uint64_t cageid, uint64_t arg1, uint64_t arg1cage,
               uint64_t arg2, uint64_t arg2cage, uint64_t arg3,
               uint64_t arg3cage, uint64_t arg4, uint64_t arg4cage,
               uint64_t arg5, uint64_t arg5cage, uint64_t arg6,
               uint64_t arg6cage) {
    int self_grate_id = getpid();

    // Overwrite the path supplied to open with a different path.
    char new_path[20] = "/tmp/redirected.txt";

    return make_threei_call(
        2 /* open */, 0,
        self_grate_id,          // self: this grate
        arg1cage,               // target: the cage that made the call
        // new_path lives in the grate's own memory -> set GRATE_MEMORY_FLAG
        (uint64_t)&new_path, self_grate_id | GRATE_MEMORY_FLAG,
        arg2, arg2cage,         // remaining args: pass through unchanged
        arg3, arg3cage,
        arg4, arg4cage,
        arg5, arg5cage,
        arg6, arg6cage,
        TRANSLATE_ERRNO_OFF     // let the originating cage handle errno
    );
}
```


## `register_handler`

```c
int register_handler(uint64_t targetcage,
    uint64_t targetcallnum,
    uint64_t this_grate_id,
    uint64_t in_grate_fn_ptr_u64);
```

Installs an interposition rule: "when cage `targetcage` issues syscall `targetcallnum`, route it to function `in_grate_fn_ptr_u64` inside grate `this_grate_id`." This is how a grate becomes part of another cage's syscall path.

**Parameters**

| Parameter | Description |
|---|---|
| `targetcage` | The cage whose syscall table is being modified (the cage whose calls you want to intercept). |
| `targetcallnum` | The syscall number to interpose on. |
| `this_grate_id` | The grate that owns the handler function — normally the caller's own cage ID. |
| `in_grate_fn_ptr_u64` | The handler's function pointer inside the grate, cast to `uint64_t`: `(uint64_t)(uintptr_t)&my_handler`. |

**Returns** `0` on success, `ELINDESRCH` if either cage is exiting. Registering the *same* (syscall, handler, grate) mapping twice is a no-op; attempting to overwrite an existing mapping with a **different** destination cage is a fatal error (threei panics to prevent accidental overwrites).

**Handler signature.** The registered function is invoked with the originating cage's ID followed by the six (value, cageid) argument pairs:

```c
int my_handler(uint64_t cageid,
               uint64_t arg1, uint64_t arg1cage,
               uint64_t arg2, uint64_t arg2cage,
               uint64_t arg3, uint64_t arg3cage,
               uint64_t arg4, uint64_t arg4cage,
               uint64_t arg5, uint64_t arg5cage,
               uint64_t arg6, uint64_t arg6cage);
```

Handlers that ignore the arguments (e.g. for `geteuid`) may declare only the leading parameters they use, as long as the grate's dispatcher casts the function pointer accordingly. The handler's return value becomes the syscall's return value in the originating cage; return `-errno` to signal an error.

**The dispatcher.** Every grate must define a function named `pass_fptr_to_wt` — the runtime looks up this export to re-enter the grate when a registered handler fires. It receives the raw function pointer plus the handler arguments, and its job is to cast and call:

```c
int pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid, uint64_t arg1,
                    uint64_t arg1cage, uint64_t arg2, uint64_t arg2cage,
                    uint64_t arg3, uint64_t arg3cage, uint64_t arg4,
                    uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage,
                    uint64_t arg6, uint64_t arg6cage) {
    if (fn_ptr_uint == 0)
        return -1;

    int (*fn)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
              uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
              uint64_t) =
        (int (*)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
                 uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
                 uint64_t))(uintptr_t)fn_ptr_uint;

    return fn(cageid, arg1, arg1cage, arg2, arg2cage, arg3, arg3cage,
              arg4, arg4cage, arg5, arg5cage, arg6, arg6cage);
}
```

Rust grates built on `grate-rs` do not need to write this — the library defines and exports `pass_fptr_to_wt` itself.

**Example** — intercept `geteuid` (syscall 107) for a child cage:

```c
int grateid = getpid();

pid_t pid = fork();
if (pid == 0) {
    int cageid = getpid();
    uint64_t fn_ptr = (uint64_t)(uintptr_t)&geteuid_grate;
    int ret = register_handler(cageid, 107, grateid, fn_ptr);
    if (ret != 0) { /* handle failure */ }

    execv(argv[1], &argv[1]);   // becomes the application cage
}
```

Note the pattern: the grate forks, the child registers handlers on *itself*, then `exec`s the application. The handler table survives `exec`, so the application's `geteuid` calls now land in the grate. See [Grates](grates.md#stacking) for how this composes into stacks.


## `copy_data_between_cages`

```c
int copy_data_between_cages(uint64_t thiscage, uint64_t targetcage,
    uint64_t srcaddr, uint64_t srccage,
    uint64_t destaddr, uint64_t destcage,
    uint64_t len, uint64_t copytype);
```

Copies memory across cage boundaries. Since each cage has its own address space, a grate cannot dereference an intercepted pointer directly; this call is the sanctioned way to pull a caller's buffer into the grate (to inspect a path, read a write buffer, ...) or to push results back out (to fill a read buffer in the caller).

**Parameters**

| Parameter | Description |
|---|---|
| `thiscage` | The calling cage's own ID (`getpid()` inside the grate). |
| `targetcage` | The cage the operation is associated with (typically the other cage involved in the copy). |
| `srcaddr`, `srccage` | Source address and the cage it belongs to. |
| `destaddr`, `destcage` | Destination address and the cage it belongs to. |
| `len` | Number of bytes to copy (for `copytype = 1`, the maximum). |
| `copytype` | `0` — raw copy of exactly `len` bytes (like `memcpy`). `1` — bounded string copy that stops at the first NUL, up to `len` bytes (like `strncpy`). Use `1` for paths and C strings, `0` for binary buffers. |

Addresses in the grate's own memory are translated automatically here — do **not** OR in `GRATE_MEMORY_FLAG`; pass plain pointers with the grate's cage ID. The source and destination ranges are permission-checked (`PROT_READ` on the source, `PROT_WRITE` on the destination) and bounds-checked before copying.

**Returns** `0` on success, `ELINDAPIABORTED` on invalid arguments, invalid copytype, or an address range that is unmapped or lacks permissions, and `ELINDESRCH` if a cage is exiting.

**Example** — an `open` handler reading the caller's path string:

```c
int open_grate(uint64_t cageid, uint64_t arg1, uint64_t arg1cage,
               uint64_t arg2, uint64_t arg2cage, ...) {
    int thiscage = getpid();
    char *pathname = malloc(256);

    // arg1 is the caller's pathname pointer, owned by cage arg1cage.
    // Copy it into this grate's buffer as a NUL-terminated string.
    copy_data_between_cages(thiscage, arg1cage,
                            arg1, arg1cage,               // src: caller's buffer
                            (uint64_t)pathname, thiscage, // dest: grate's buffer
                            256, 1 /* string copy */);

    printf("[grate] open(\"%s\")\n", pathname);
    free(pathname);
    /* ... forward or handle the call ... */
}
```

The reverse direction works the same way — e.g. a `read` handler filling the caller's buffer copies *from* a grate buffer (`srccage = thiscage`) *to* the caller's pointer (`destaddr = arg2, destcage = arg2cage`) with `copytype = 0`.


## `copy_handler_table_to_cage`

```c
int copy_handler_table_to_cage(uint64_t srccage, uint64_t targetcage);
```

Copies `srccage`'s entire syscall handler table to `targetcage`, so the target cage's syscalls are routed identically from then on.

The runtime already uses this internally during `fork` so that children inherit their parent's routing (see [Grates — inheritance](grates.md#inheritance-properties)). A grate calls it explicitly when it creates or discovers a cage that should be subject to the
same interposition rules as an existing one — for example, to stamp a template cage's routing onto new cages, instead of re-issuing many individual `register_handler` calls.

**Parameters**

| Parameter | Description |
|---|---|
| `srccage` | The cage whose handler table is copied (usually the caller). |
| `targetcage` | The cage that receives the copy. |

**Returns** `0` on success, `ELINDAPIABORTED` if `srccage` has no handler
table, `ELINDESRCH` if either cage is exiting.


## Putting it together: a minimal grate

```c
#include <lind_syscall.h>
#include <stdio.h>
#include <sys/wait.h>
#include <unistd.h>

// 1. The dispatcher — required in every grate, always the same shape.
int pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid, uint64_t arg1,
                    uint64_t arg1cage, uint64_t arg2, uint64_t arg2cage,
                    uint64_t arg3, uint64_t arg3cage, uint64_t arg4,
                    uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage,
                    uint64_t arg6, uint64_t arg6cage) {
    if (fn_ptr_uint == 0)
        return -1;
    int (*fn)(uint64_t) = (int (*)(uint64_t))(uintptr_t)fn_ptr_uint;
    return fn(cageid);
}

// 2. The handler(s).
int geteuid_grate(uint64_t cageid) {
    printf("[grate] geteuid intercepted for cage %llu\n", cageid);
    return 10;  // becomes the caller's geteuid() result
}

// 3. main: fork, register handlers on the child, exec the application.
int main(int argc, char *argv[]) {
    int grateid = getpid();

    pid_t pid = fork();
    if (pid == 0) {
        int cageid = getpid();
        register_handler(cageid, 107 /* geteuid */, grateid,
                         (uint64_t)(uintptr_t)&geteuid_grate);
        execv(argv[1], &argv[1]);
    }

    int status;
    while (wait(&status) > 0)
        ;
    return 0;
}
```

Build a C grate with:

```bash
lind_compile -s --compile-grate my-grate.c
```

More complete, runnable C examples live in [`tests/grate-tests/`](https://github.com/Lind-Project/lind-wasm/tree/main/tests/grate-tests), including argument rewriting (`simple-tests/diff-cage-args_grate.c`), cross-cage copies (`copy-data-between-cages/`), and interposing on process-control calls (`interposing-calls/`).


## Writing grates in Rust (`grate-rs`)

Rust grates live alongside C grates in the [lind-wasm-example-grates](https://github.com/Lind-Project/lind-wasm-example-grates) repository. The [`grate-rs`](https://github.com/Lind-Project/lind-wasm-example-grates/tree/main/lib/grate-rs) library binds the same four sysroot symbols documented above (`make_threei_call`, `register_handler`, `copy_data_between_cages`, `copy_handler_table_to_cage`) and wraps them in safe, `Result`-based APIs. Everything on this page about cage IDs, (value, cageid) pairs, `GRATE_MEMORY_FLAG`, copytypes, and error codes applies identically.

### What the library gives you on top of the raw C API:

- **`GrateBuilder`** — handles the whole fork / register / exec / wait lifecycle. Unlike the C pattern above (where the child registers handlers on itself before `exec`), the builder registers handlers from the *parent* grate process, using a shared semaphore to hold the child back until registration is complete. Optional `preexec` and `teardown` hooks run around the cage's lifetime.
- **`pass_fptr_to_wt`** — the required dispatcher is defined and exported by the library; you never write it.
- **`SyscallHandler`** — the handler signature as a Rust type: an `extern "C" fn(cageid: u64, arg1: u64, arg1cage: u64, ..., arg6: u64, arg6cage: u64) -> i32`.
- **Safe wrappers** — `register_handler`, `copy_data_between_cages`, `copy_handler_table_to_cage` return `Result<(), GrateError>`, and `make_threei_call` returns `Result<i32, GrateError>`; `getcageid()` returns the current cage ID.
- **Syscall number constants** — e.g. `grate_rs::constants::SYS_GETEUID`, so you don't hard-code raw numbers.
- **fd-table support** — `GrateBuilder::enable_fd_translate_policy` / `register_default_fd_handlers_except` install default handlers for fd-related syscalls, for grates that virtualize file descriptors.

The minimal geteuid grate from the previous section, in Rust:

```rust
use grate_rs::{GrateBuilder, GrateError};
use grate_rs::constants::SYS_GETEUID;

extern "C" fn my_handler(
    _cageid: u64,
    _arg1: u64, _arg1cage: u64, _arg2: u64, _arg2cage: u64,
    _arg3: u64, _arg3cage: u64, _arg4: u64, _arg4cage: u64,
    _arg5: u64, _arg5cage: u64, _arg6: u64, _arg6cage: u64,
) -> i32 {
    10  // becomes the caller's geteuid() result
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    GrateBuilder::new()
        .register(SYS_GETEUID, my_handler)
        .teardown(|result| println!("done: {:?}", result))
        .run(argv);  // forks, registers, execs argv[0], waits — never returns
}
```

Inside a handler, the raw wrappers work just like their C counterparts. For example, copying the caller's path string into the grate (compare with the C `copy_data_between_cages` example above):

```rust
let thiscage = grate_rs::getcageid();
let mut path = [0u8; 256];
grate_rs::copy_data_between_cages(
    thiscage, arg1cage,
    arg1, arg1cage,                       // src: caller's buffer
    path.as_mut_ptr() as u64, thiscage,   // dest: grate's buffer
    256, 1,                               // copytype 1 = string copy
)?;
```

Build a Rust grate with:

```bash
cargo lind_compile
```

See the [lind-wasm-example-grates README](https://github.com/Lind-Project/lind-wasm-example-grates/blob/main/README.md) for repository layout, build, and test instructions, and `rust-grates/` there for complete examples.
