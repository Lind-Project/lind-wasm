# Building safeposix-rust

## Setting environment

We need to set environment with the following codes

```
cd /home
apt update
apt install git
apt install curl
apt install gcc
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs/ | sh
rustup install nightly
. "$HOME/.cargo/env"
rustup default nightly
```

## Clone git repo

First we need to clone the mono-repo:

```sh
git clone https://github.com/Lind-Project/lind-wasm.git
```

Then, go the `RawPOSIX` directory:

```sh
cd lind-wasm/src/RawPOSIX
```

## Build

Compile and make sure there are librustposix.so

```
cargo build
```
