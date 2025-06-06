# syntax=docker/dockerfile:1.7-labs
# (use non-stable syntax for convenient --parents option in COPY command)

# Download clang
# NOTE: chmod is required to extract as user below
FROM scratch AS clang
ADD --chmod=644 https://github.com/llvm/llvm-project/releases/download/llvmorg-16.0.4/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04.tar.xz /clang.tar.xz

FROM ubuntu:latest

#####################
# SYSTEM DEPENDENCIES
#####################
# TODO: only install required deps, and only in the stage, where needed
# TODO: https://docs.docker.com/build/building/best-practices/#apt-get
RUN apt-get update && \
    apt-get install -y -qq \
        apt-transport-https \
        bash \
        binaryen \
        bison \
        build-essential \
        curl \
        g++ \
        g++-i686-linux-gnu \
        gawk \
        gcc \
        gcc-i686-linux-gnu \
        git \
        gnupg \
        golang \
        libssl-dev \
        libxml2 \
        openssl \
        python3 \
        sudo \
        unzip \
        vim \
        wget \
        zip

#####################
# USER SETUP
#####################
# TODO: do not require user and abspaths (in build/test scripts)
RUN usermod --login lind --move-home --home /home/lind ubuntu && \
    groupmod --new-name lind ubuntu
RUN echo "lind ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers
USER lind
RUN mkdir /home/lind/lind-wasm
WORKDIR /home/lind/lind-wasm


###################
# USER DEPENDENCIES
###################
# Install pinned rust nightly version (known to work)
# TODO: Figure out why newer versions break the build and unpin
# TODO: Beware of RUN layer caching: cache not invalidated by remote change
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --default-toolchain nightly-2025-06-01
ENV PATH="/home/lind/.cargo/bin:${PATH}"

# Extract Clang
# see best practices for downloading and extracting large files
# https://docs.docker.com/build/building/best-practices/#add-or-copy
RUN --mount=from=clang,target=/clang tar xf /clang/clang.tar.xz

###################
# Build GLIBC
###################
COPY --chown=lind:lind src/glibc src/glibc
RUN ./src/glibc/gen_sysroot.sh

###################
# Build WASMTIME
###################
COPY --chown=lind:lind --parents src/wasmtime src/RawPOSIX src/fdtables src/sysdefs .
RUN cargo build --manifest-path src/wasmtime/Cargo.toml

###################
# Run TESTS
###################
COPY --chown=lind:lind --parents scripts tests tools .
RUN ./scripts/wasmtestreport.py
