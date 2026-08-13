# Getting Started

By the end of this page you will have compiled a C program to WebAssembly against
*lind-glibc* and run it inside a Lind cage.

If you want the concepts first — cages, grates, 3i, RawPOSIX — read the
[Basics](index.md). If you are here to contribute, also see the
[Contributor Instructions](contribute/index.md).

There are two ways to get a working Lind:

- **[Option A: the prebuilt development image](#option-a-prebuilt-development-image)** —
  Docker, no build step. Start here.
- **[Option B: build from source](#option-b-build-from-source)** — build the image
  yourself, or install natively on Linux.

## Prerequisites

**Platform.** Lind-Wasm targets **linux/amd64 only**. This is why every `docker`
command below passes `--platform=linux/amd64`. On an arm64 host — an Apple
Silicon Mac, for example — the image still runs under emulation, but noticeably
more slowly. A *native* (non-Docker) install requires x86-64 Linux.

**Disk.** Budget **~20 GB free**. The toolchain is large: clang+LLVM and the
Wasmtime build account for most of it.

**Privileges.** Running a program needs root. `lind_run` re-executes itself under
`sudo` because the runtime `chroot`s into the Lind filesystem, so expect a
password prompt the first time.

**Reference environment.** The setup is tested on Ubuntu 22.04, x86-64, 16 GB RAM.

To check a machine against all of the above at once:

```bash
make checkenv
```

Each prerequisite is reported as `OK` or `FAIL`, and every `FAIL` comes with the
command that fixes it. On a fresh checkout that has not been built yet, use
`make checkenv BUILD_ONLY=1` to skip the checks for build outputs.

## Option A: prebuilt development image

This is the fastest path and the one to use if you are evaluating Lind.

```bash
docker pull --platform=linux/amd64 securesystemslab/lind-wasm-dev  # this might take a while ...
docker run --platform=linux/amd64 -it --privileged --ipc=host --init \
  --cap-add=SYS_PTRACE securesystemslab/lind-wasm-dev /bin/bash
cd lind-wasm
```

What those flags are for:

| Flag | Why |
| --- | --- |
| `--platform=linux/amd64` | see [Prerequisites](#prerequisites) |
| `--privileged` | the runtime `chroot`s into `lindfs` and runs as root |
| `--ipc=host` | shared-memory system calls |
| `--init` | reaps the processes that `fork`/`exec` tests leave behind |
| `--cap-add=SYS_PTRACE` | attaching `gdb`, `strace` or `perf` |

!!! note "There is no build step"

    This image ships a runtime, sysroot and `lindfs` that were built when the
    image was built (see `RUN make lind-debug` in
    [`Docker/Dockerfile.dev`](https://github.com/Lind-Project/lind-wasm/blob/main/Docker/Dockerfile.dev)).
    Skip straight to [Run your first program](#run-your-first-program).

    You only need to rebuild after changing the source — see
    [Option B](#option-b-build-from-source).

## Option B: build from source

### B1. Build the development image yourself

Useful for building a specific branch. See
[Development setup](contribute/dev-container.md) for the `docker build`
invocation and its build args.

### B2. Install natively on Linux

Lind-Wasm builds and runs directly on Ubuntu 22.04, both native and under WSL2.
The [Native Linux setup](contribute/running-on-native-linux.md) guide covers the
dependencies, pinned toolchain versions and environment variables.

### Building

Either way, one command builds everything:

```bash
make build
```

That builds the runtime (`lind-boot`, Wasmtime, RawPOSIX, 3i), the *lind-glibc*
sysroot, and the `lindfs` skeleton the runtime `chroot`s into.

!!! warning "Use `make build`, not the individual targets"

    `make lind-boot sysroot` looks equivalent but skips the `lindfs` target, so
    `lindfs/etc`, `dev/null`, the locale data and the timezone database are never
    created, and programs fail in confusing ways. `make sysroot` on its own fails
    outright on a clean checkout, because building the shared libc needs
    `lind-boot` to already exist. `make all` is an alias for `make build`.

    For a debug runtime, use `make lind-debug` instead — but do not mix the two,
    see [Troubleshooting](#troubleshooting). Other targets and build knobs
    (`FDTABLES_IMPL`, `NO_LOGGING`, `WITH_FPCAST`) are documented in the
    [`Makefile`](https://github.com/Lind-Project/lind-wasm/blob/main/Makefile).

!!! danger "The checkout must live at `/home/lind/lind-wasm`"

    The path the runtime `chroot`s into is currently compiled in as a constant
    (`LINDFS_ROOT` in `src/sysdefs/src/constants/lind_platform_const.rs`), so a
    checkout anywhere else panics at startup with
    `The configured lindfs does not exist`. If yours is elsewhere, symlink it:

    ```bash
    sudo mkdir -p /home/lind
    sudo ln -sfnT "$PWD" /home/lind/lind-wasm
    ```

    `make checkenv` checks this for you. This is a known limitation, not a design
    goal; making the path configurable is tracked upstream.

## Run your first program

Write a C program:

```bash
cat << EOF > hello.c
#include <stdio.h>

int main() {
    printf("Hello, World!\n");
    return 0;
}
EOF
```

Compile and run it:

```bash
lind-clang hello.c
lind-wasm hello.cwasm
```

```text
Hello, World!
```

`lind-clang` and `lind-wasm` are the names the development image installs. Outside
the container, call the scripts directly — `scripts/bin/lind_compile` and
`scripts/bin/lind_run` — or symlink them onto your `PATH` as the
[Native Linux setup](contribute/running-on-native-linux.md) guide describes.

### What just happened

1. `lind_compile` compiled `hello.c` into a WebAssembly binary linked against
   *lind-glibc*, optimized it with Lind's custom `wasm-opt`, and ahead-of-time
   compiled the result to `hello.cwasm`. The output was copied into the Lind
   filesystem root, `lindfs/`.
2. `lind_run` executed it on the *Lind-Wasm* runtime, with system calls mediated
   by *3i* and serviced by the *RawPOSIX* microvisor.

Two details worth knowing early:

- **Paths are relative to `lindfs`, not your shell.** The runtime `chroot`s into
  `lindfs` and changes directory to `/`, so `hello.cwasm` and `/hello.cwasm` both
  refer to `lindfs/hello.cwasm`. Your host working directory is invisible to the
  program.
- **`lind_compile` builds dynamically by default**, producing a position-independent
  executable that resolves *lind-glibc* from `lindfs/lib/libc.cwasm` at run time.
  Pass `-s` for a traditional statically linked binary instead. Both run the same
  way; the static build does not need the shared libc to be present.

## Troubleshooting

**`The configured lindfs does not exist: /home/lind/lind-wasm/lindfs`** —
your checkout is not at the compiled-in path. Create the symlink shown in
[Option B](#option-b-build-from-source).

**`lind-clang: command not found`** — those names exist only inside the
development image. Use `scripts/bin/lind_compile` and `scripts/bin/lind_run`, or
add the symlinks.

**An unexpected password prompt when running a program** — expected. `lind_run`
re-executes itself under `sudo -E` because the runtime needs root to `chroot`.

**`WARNING: The requested image's platform (linux/amd64) does not match the
detected host platform`** — you are on an arm64 host. Add
`--platform=linux/amd64`; the image runs under emulation, more slowly.

**`unknown import: debug::lind_debug_num`** — a debug-built sysroot is being used
with a release runtime, or the reverse. Rebuild consistently: `make build` for
release, `make lind-debug` for debug — not one after the other.

**`[LIND DEBUG NUM]` / `[LIND DEBUG STR]` lines before your output** — not errors.
The published development image is built with `make lind-debug`, which enables
debug logging.

If none of these match, please
[open an issue](https://github.com/Lind-Project/lind-wasm/issues) — including the
output of `make checkenv` makes it much easier to help.

## What's next

- Compile a Rust crate against *lind-glibc*:
  [Compiling Rust programs](contribute/compile-with-rust.md)
- Run the test suite: [Testing](contribute/testing.md)
- Understand the pieces: [Internal Documentation](internal/index.md)
- Contribute a change: [Contributor Instructions](contribute/index.md)
