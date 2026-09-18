# Development setup

To access an environment with the source code and tooling, there is a development image available as well.  
(Note: If you intend to use perf, you will need to install the appropriate `linux-tools-xxx` for your kernel)

> **Note:** Despite the page name, this is a plain Docker image — there is no
> `.devcontainer/` in the repository, so VS Code's "Reopen in Container" will not
> pick it up automatically.

The published image is prebuilt: it already contains a compiled runtime, sysroot
and `lindfs`, so you can run a program without building anything. See
[Getting Started](../getting-started.md) for the quickest path.

```
docker pull --platform=linux/amd64 securesystemslab/lind-wasm-dev # this might take a while ...
docker run --platform=linux/amd64 -it --privileged --ipc=host --init --cap-add=SYS_PTRACE securesystemslab/lind-wasm-dev /bin/bash
```

This container can be built locally and the following args can be varied at build time for use on any branch or
configuration needed.

```
docker build \
  --platform=linux/amd64 \
  --build-arg USERNAME=lind \
  --build-arg BRANCH_NAME=main \
  --build-arg LLVM_VERSION=llvmorg-18.1.8 \
  --build-arg CLANG_PACKAGE=clang+llvm-18.1.8-x86_64-linux-gnu-ubuntu-18.04 \
  -f ./Docker/Dockerfile.dev \
  -t lind-dev .
```
The build process can be quite long, depending on system resources on the build machine.  
You can then run it with:

```
docker run --platform=linux/amd64 -it --privileged --ipc=host --init --cap-add=SYS_PTRACE lind-dev /bin/bash
```