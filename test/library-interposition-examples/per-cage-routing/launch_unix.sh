#!/bin/sh

set -e

~/lind-wasm/src/lind-boot/target/release/lind-remote-server ./server_unix_patched.json
