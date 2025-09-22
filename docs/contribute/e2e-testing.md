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

##Usage C — create a base image and mount the source

`docker build --platform=linux/amd64 -f Docker/Dockerfile.e2e -t dev --target base .`

`docker run --platform=linux/amd64 -v $(PWD):/lind -w /lind -it dev /bin/bash`

- Use the **`base`** stage to match CI dependencies while keeping your source outside the image for fast, iterative editing.

- Inside the container, run `make build && make test` to mirror CI exactly; the command exits non-zero on any e2e failure.

---

##CI overview (how e2e is wired)

Workflow: .github/workflows/e2e.yml

High level:

- Set up Docker Buildx for linux/amd64

- Build Docker/Dockerfile.e2e with GitHub Actions cache

- Execute the test stage (same behavior as Usage A)

---

##Troubleshooting

- Apple Silicon / wrong arch → add --platform=linux/amd64 to all build/run commands.

- Slow first build → increase Docker memory (≈ 6–8 GB) to accommodate sysroot/wasmtime builds.

- Stale artifacts → run make clean/make distclean and rebuild inside the dev (base) container.


