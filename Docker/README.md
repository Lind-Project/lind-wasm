### Dockerfile.mnt.dev commands

Build (from within repo main directory):

docker build \
  --platform=linux/amd64 \
  --build-arg USERNAME=lind \
  -f ./Docker/Dockerfile.mnt.dev \
  -t lind-mnt-dev .


Run (from within repo main directory):

docker run \
  --platform=linux/amd64 \
  -it \
  --privileged \
  --ipc=host \
  --init \
  --cap-add=SYS_PTRACE \
  -v $(pwd):/home/lind/lind-wasm \
  -v $(pwd)/../lind-wasm-example-grates:/home/lind/lind-wasm-example-grates \
  -v $(pwd)/../scratch:/home/lind/scratch \
  -v $(pwd)/../glibc-new:/home/lind/glibc-new \
  -v $(pwd)/../glibc-2.35:/home/lind/glibc-2.35 \
  -v $(pwd)/../gsfork-prototype/rewriter-v2:/home/lind/rewriter \
  lind-mnt-dev \
  /bin/bash

For WASM only:

docker run \
  --platform=linux/amd64 \
  -it \
  --privileged \
  --ipc=host \
  --init \
  --cap-add=SYS_PTRACE \
  -v $(pwd):/home/lind/lind-wasm \
  -v $(pwd)/../lind-wasm-example-grates:/home/lind/lind-wasm-example-grates \
  lind-mnt-dev-frsh \
  /bin/bash