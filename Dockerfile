# Use an official Ubuntu as a parent image
FROM --platform=linux/amd64 ubuntu:latest

# Set the working directory to home
WORKDIR /home

# Install all the required dependencies
RUN apt-get update && \
    apt-get install -y build-essential git wget gcc-i686-linux-gnu g++-i686-linux-gnu \
    bison gawk vim libxml2 python3 curl gcc binaryen

# Clone the Lind-wasm repository
RUN git clone https://github.com/Lind-Project/lind-wasm.git

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y && \
    . "$HOME/.cargo/env" && \
    rustup install nightly && \
    rustup default nightly

# Ensure the Rust environment is available in future RUN instructions
ENV PATH="/root/.cargo/bin:${PATH}"

# Install clang-16 for compiling the code
RUN wget https://github.com/llvm/llvm-project/releases/download/llvmorg-16.0.4/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04.tar.xz && \
    tar -xf clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04.tar.xz && \
    mv clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04 lind-wasm/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04

# Go inside Lind-wasm repository and clone rustposix
WORKDIR /home/lind-wasm

# Move wasi directory
RUN mv ./src/glibc/wasi ./clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04/lib/clang/16/lib

ENV CLANG="/home/lind-wasm/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04"

# Build Lind-wasm
#RUN chmod +x lindtool.sh
RUN ./lindtool.sh make_all
RUN ./lindtool.sh compile_wasmtime

CMD [ "bash" ]
