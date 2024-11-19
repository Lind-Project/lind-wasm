# RustPOSIX [![Build Status](https://github.com/Lind-Project/safeposix-rust/actions/workflows/lind-selfhost.yml/badge.svg?branch=develop)](https://github.com/Lind-Project/safeposix-rust/actions/workflows/lind-selfhost.yml)

More implementation details could be found at [wiki](https://github.com/Lind-Project/lind-docs/blob/main/docs/RustPOSIX/Home.md).

## Contents

* [Home](https://github.com/Lind-Project/lind-docs/blob/main/docs/RustPOSIX/Home.md)
* [Architecture](https://github.com/Lind-Project/lind-docs/blob/main/docs/RustPOSIX/Architecture.md)
* [Interface](https://github.com/Lind-Project/lind-docs/blob/main/docs/RustPOSIX/Interface.md)
* [Run Independently](https://github.com/Lind-Project/lind-docs/blob/main/docs/RustPOSIX/Run-Independently.md)
* [Security Model](https://github.com/Lind-Project/lind-docs/blob/main/docs/RustPOSIX/Security-Model.md)
* [Style Guide](https://github.com/Lind-Project/lind-docs/blob/main/docs/RustPOSIX/Style-Guide.md)
* [Testing and Debugging](https://github.com/Lind-Project/lind-docs/blob/main/docs/RustPOSIX/Testing-and-Debugging.md)

## Run RustPOSIX-Rust

Quick start
Use Develop branch for the most stable behaviour.

```bash
docker build -t --platform <your platform> <image_name> .devcontainer
docker run -it <image_name>

```

This will create a quick container with rustposix build at your local changes.
helpful for exploration and easy testing.

See reference at [Run RustPOSIX Independently](https://github.com/Lind-Project/lind-docs/blob/main/docs/RustPOSIX/Run-Independently.md)

## Test RustPOSIX-Rust

Use main branch for the most stable behaviour.

```bash
cargo build
cargo test --lib
```

See reference at [Testing and Debugging](https://github.com/Lind-Project/lind-docs/blob/main/docs/RustPOSIX/Testing-and-Debugging.md)

## Development Guideline

* All PRs should be merged to the Develop branch

* Any imports from the standard library or any crates should be done in an interface file

More detailed guideline will be in [RustPOSIX's wiki](https://github.com/Lind-Project/lind-docs/blob/main/docs/RustPOSIX/Style-Guide.md)