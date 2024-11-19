# Use an official Rust image as a parent image
FROM ubuntu:latest
ARG DEBIAN_FRONTEND=noninteractive
ARG arch=x86_64

# Install tools and dependencies for building Rust projects
RUN apt-get update && apt-get install -y \
    build-essential \
    pkg-config \
    curl \
    libssl-dev \
    git
# Install Rust from source
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y
# Add cargo bin to PATH
ENV PATH="/root/.cargo/bin:${PATH}"

# Install the nightly Rust toolchain
RUN rustup toolchain install nightly

# Set the default toolchain to nightly
RUN rustup default nightly

# Verify installation
RUN rustc --version

