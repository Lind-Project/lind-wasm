# Use an official Ubuntu as a parent image
FROM ubuntu:latest
ARG DEV_MODE
ENV DEV_MODE=${DEV_MODE:-false}

ARG BRANCH_NAME
ENV BRANCH_NAME=${BRANCH_NAME:-main}

ARG GLIBC_TAR_CACHE
ENV GLIBC_TAR_CACHE=${GLIBC_TAR_CACHE:-false}

# Install all the required dependencies
RUN apt-get -qq update && \
    apt-get install -y -qq build-essential git wget gcc-i686-linux-gnu g++-i686-linux-gnu \
    bison gawk vim libxml2 python3 python3-pip python3-jinja2 curl gcc g++ binaryen unzip zip golang bash openssl libssl-dev \
    sudo

RUN apt install -qq apt-transport-https curl gnupg -y && \
    curl -fsSL https://bazel.build/bazel-release.pub.gpg | gpg --dearmor >bazel-archive-keyring.gpg && \
    mv bazel-archive-keyring.gpg /usr/share/keyrings && \
    echo "deb [arch=amd64 signed-by=/usr/share/keyrings/bazel-archive-keyring.gpg] https://storage.googleapis.com/bazel-apt stable jdk1.8" | tee /etc/apt/sources.list.d/bazel.list

RUN apt update -qq && apt install -y -qq bazel
RUN go install github.com/bazelbuild/buildtools/buildifier@latest

ARG USERNAME=lind

RUN usermod --login ${USERNAME} --move-home --home /home/${USERNAME} ubuntu && \
    groupmod --new-name ${USERNAME} ubuntu
RUN echo "lind ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

USER lind

WORKDIR /home/lind

# Install Rust
RUN curl --silent --show-error --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y && \
    . "$HOME/.cargo/env" && \
    rustup install nightly && \
    rustup default nightly

RUN go install github.com/bazelbuild/buildtools/buildifier@latest
    
# Ensure the Rust environment is available in future RUN instructions
ENV PATH="/home/lind/.cargo/bin:/home/lind/go/bin:${PATH}"

ENV CLANG="/home/lind/lind-wasm/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04"

### non-DEV_MODE Commmands begin here

WORKDIR /home/lind
# Clone the Lind-wasm repository
RUN if [ "$DEV_MODE" != "true" ]; then git clone --branch "$BRANCH_NAME" --single-branch https://github.com/Lind-Project/lind-wasm.git; fi
# Install clang-16 for compiling the code
RUN if [ "$DEV_MODE" != "true" ]; then wget -q https://github.com/llvm/llvm-project/releases/download/llvmorg-16.0.4/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04.tar.xz && \
    tar -xf clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04.tar.xz && \
    mv clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04 lind-wasm/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04; fi
WORKDIR /home/lind/lind-wasm
# Move wasi directory
RUN if [ "$DEV_MODE" != "true" ]; then cp -r ./src/glibc/wasi ./clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04/lib/clang/16/lib; fi
# Build Lind-wasm
#RUN chmod +x lindtool.sh
# RUN if [ "$DEV_MODE" != "true" ]; then bazel build //:make_all; fi
RUN if [ "$DEV_MODE" != "true" ]; then \
      if [ "$GLIBC_TAR_CACHE" = "false" ]; then \
        echo "------------------------" && \
        echo "Building glibc" && \
        echo "------------------------" && \
        bazel build //:make_glibc; \
      else \
        echo "------------------------" && \
        echo "Using cached glibc" &&  \
        echo "------------------------" && \
        /home/lind/lind-wasm/src/glibc/gen_sysroot.sh; \
      fi; \
    fi
RUN if [ "$DEV_MODE" != "true" ]; then bazel build //:make_wasmtime; fi

### non-DEV_MODE Commmands end here

SHELL ["/bin/bash", "-c"]
