# End-to-End testing

Multi-stage **.e2e** flow for lind-wasm **end-to-end testing and image creation**.

- Installs build dependencies
- Builds **wasmtime**, **glibc**, and a **sysroot** for clang cross-compilation
- **A.** Runs end-to-end tests (default)
- **B.** Creates a Docker image with the lind-wasm toolchain
- **C.** Provides a base image for interactive development with the full source tree mounted

> **NOTE**  
> The **`test` stage (A)** runs end-to-end tests on `docker build` and is optimized for build time and caching. It is **not meant** for `docker run`.  

> Use the **`release` stage (B)** to create an image that includes the full lind-wasm toolchain (for demos, experiments, etc.).  

> For development, you may want to build just the **`base` stage (C)** and mount the full source tree.

> The **Dev** image is automatically rebuilt weekly from `Docker/Dockerfile.dev` on `main` and pushed to Docker Hub as `securesystemslab/lind-wasm-dev:latest`.


---

## Usage A — test

### From repo root
`docker build --platform=linux/amd64 -f Docker/Dockerfile.e2e .`

- Triggers the default test stage.

- Earlier stages build prerequisites (clang/LLVM, Rust, wasmtime, sysroot).

- The test stage executes make test during the build and fails the build on any test failure.

- Check the build log for the e2e report (the harness prints results.json).

---

## Usage B — create and run a toolchain image (release)

### Build a runnable toolchain image
`docker build --platform=linux/amd64 -f Docker/Dockerfile.e2e -t release --target release .`

### Run image
`docker run --platform=linux/amd64 -it release /bin/bash`

- Contains the lind-wasm toolchain (wasmtime + sysroot + scripts/tests).

- Intended for demos/experiments.

- Not designed to run make test directly (the Makefile isn’t copied here).

---

## Usage C — create a base image and mount the source

`docker build --platform=linux/amd64 -f Docker/Dockerfile.e2e -t dev --target base .`

`docker run --platform=linux/amd64 -v $(PWD):/lind -w /lind -it dev /bin/bash`

- Use the **`base`** stage to match CI dependencies while keeping your source outside the image for fast, iterative editing.

- Inside the container, run `make build && make test` to mirror CI exactly; the command exits non-zero on any e2e failure.

---

## Build steps

### make sysroot

Runs `scripts/make_glibc_and_sysroot.sh` to:

- Configure & build glibc (WASM/WASI target) and compile additional NPTL/syscall bits and tiny ASM stubs.

- Collect selected `.o` objects (excluding objects defining `main`) and archive them into `src/glibc/sysroot/lib/wasm32-wasi/libc.a`; creates `libpthread.a`; installs headers under `src/glibc/sysroot/include/wasm32-wasi/`; and copies `crt1.o`.



### make wasmtime

- Builds the embedded Wasmtime with Cargo (release) from `src/wasmtime/`.


### make test

Runs `scripts/wasmtestreport.py` which:

- Discovers tests from the repository’s test trees and honors `skip_test_cases.txt`.

- Organizes results (e.g., deterministic / non_deterministic groups), writes `results.json` (and may render an HTML summary if enabled).

- The Makefile prints `results.json` and fails when any failures are present.


---

## CI overview (how e2e is wired)

Workflow: `.github/workflows/e2e.yml`

High level:

- Set up Docker Buildx for linux/amd64

- Build Docker/Dockerfile.e2e with GitHub Actions cache

- Execute the test stage (same behavior as Usage A)

---

## Caching

### What we cache

- Buildx GHA layer cache (`cache-from/to: type=gha`) for apt/clang/rust/tooling and per-stage layers.

- Multi-stage outputs: `build-wasmtime` and `build-glibc` are mounted into `test`, so tests avoid rebuilding toolchains.

### GitHub Actions semantics

- `cache-from: type=gha` pulls layers from the hosted GitHub Actions cache.

- `cache-to: type=gha,mode=max` pushes all reusable layers (not just the final image).

- The first cold run builds all layers; subsequent runs only rebuild changed layers, dramatically speeding up CI.

> Note: GitHub Actions may evict cached layers over time; when that happens a run starts cold but still passes the same way.


### Local build with cache

#### One-time: create/select a builder
`docker buildx create --use --name lind-builder || docker buildx use lind-builder`

#### Build with cache import/export
`docker buildx build --platform=linux/amd64 -f Docker/Dockerfile.e2e --cache-from type=local,src=~/.cache/docker-buildx --cache-to type=type=local,dest=.~/.cache/docker-buildx,mode=max .`

> CI typically uses type=gha; locally a local cache is simple and reliable.

---

## Troubleshooting

- Apple Silicon / wrong arch → add --platform=linux/amd64 to all build/run commands.

- Slow first build → increase Docker memory (≈ 6–8 GB) to accommodate sysroot/wasmtime builds.

- Stale artifacts → run make clean/make distclean and rebuild inside the dev (base) container.

- Cache debugging → add --progress=plain to docker build to see which step invalidated.


