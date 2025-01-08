# Use an official Ubuntu as a parent image
FROM ubuntu:latest

# Install all the required dependencies
RUN apt-get update && \
    apt-get install -y build-essential git wget gcc-i686-linux-gnu g++-i686-linux-gnu \
    bison gawk vim libxml2 python3 curl gcc g++ binaryen unzip zip golang bash

RUN apt install apt-transport-https curl gnupg -y && \
    curl -fsSL https://bazel.build/bazel-release.pub.gpg | gpg --dearmor >bazel-archive-keyring.gpg && \
    mv bazel-archive-keyring.gpg /usr/share/keyrings && \
    echo "deb [arch=amd64 signed-by=/usr/share/keyrings/bazel-archive-keyring.gpg] https://storage.googleapis.com/bazel-apt stable jdk1.8" | tee /etc/apt/sources.list.d/bazel.list

RUN apt update && apt install -y bazel
RUN go install github.com/bazelbuild/buildtools/buildifier@latest



ARG USERNAME=lind

RUN usermod --login ${USERNAME} --move-home --home /home/${USERNAME} ubuntu && \
    groupmod --new-name ${USERNAME} ubuntu
RUN echo "lind ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers


USER lind

WORKDIR /home/lind

# Clone the Lind-wasm repository
RUN git clone https://github.com/Lind-Project/lind-wasm.git

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y && \
    . "$HOME/.cargo/env" && \
    rustup install nightly && \
    rustup default nightly

RUN go install github.com/bazelbuild/buildtools/buildifier@latest
    
# Ensure the Rust environment is available in future RUN instructions
ENV PATH="/home/lind/.cargo/bin:/home/lind/go/bin:${PATH}"

# Install clang-16 for compiling the code
RUN wget https://github.com/llvm/llvm-project/releases/download/llvmorg-16.0.4/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04.tar.xz && \
    tar -xf clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04.tar.xz && \
    mv clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04 lind-wasm/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04

# Go inside Lind-wasm repository and clone rustposix
WORKDIR /home/lind/lind-wasm

# Move wasi directory
RUN mv ./src/glibc/wasi ./clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04/lib/clang/16/lib

ENV CLANG="/home/lind/lind-wasm/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04"

# Build Lind-wasm
#RUN chmod +x lindtool.sh
RUN bazel build //:make_all
RUN bazel build //:make_wasmtime


SHELL ["/bin/bash", "-c"]
