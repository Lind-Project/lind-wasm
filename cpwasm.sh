#!/bin/bash

LIND_ROOT="/home/lind/lind-wasm-3i/src/RawPOSIX/tmp"

# Get the lind_root reference from the user input
for arg in "$@"; do
  case $arg in
    --lind_root=*)
      LIND_ROOT="${arg#*=}"
      shift
      ;;
    *)
      echo "Unknown argument: $arg"
      exit 1
      ;;
  esac
done

cd /home/lind/lind-wasm-3i/src/wasmtime
export LIND_ROOT
echo "LIND_ROOT is set to: $LIND_ROOT"
cargo build
