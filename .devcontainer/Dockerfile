# Use an official Ubuntu as a parent image
FROM ubuntu:latest
ARG DEV_MODE
ENV DEV_MODE=${DEV_MODE:-false}

ARG GLIB_TAR_CACHE
ARG USERNAME=lind

# Install all the required dependencies
RUN apt-get -qq update && \
    apt-get install -y -qq build-essential git wget gcc-i686-linux-gnu g++-i686-linux-gnu \
    bison gawk vim libxml2 python3 curl gcc g++ binaryen unzip zip golang bash

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
    
# Ensure the Rust environment is available in future RUN instructions
ENV PATH="/home/lind/.cargo/bin:/home/lind/go/bin:${PATH}"

ENV CLANG="/home/lind/lind-wasm/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04"

### non-DEV_MODE Commmands begin here

WORKDIR /home/lind
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
      if [ "$GLIB_TAR_CACHE" != "true" ]; then \
        /home/lind/lind-wasm/tests/build.sh make_glibc \
      fi; \
    fi
RUN if [ "$DEV_MODE" != "true" ]; then  /home/lind/lind-wasm/tests/build.sh make_wasmtime; fi

### non-DEV_MODE Commmands end here

SHELL ["/bin/bash", "-c"]
