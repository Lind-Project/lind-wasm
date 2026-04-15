#!/usr/bin/env python3
# Auto-generates syscall mapping constants for lind-wasm.
#
# Reads syscall definitions from glibc's lind_syscall_num.h and generates:
#  - src/sysdefs/src/constants/syscall_const.rs (full set of syscalls)
#
# This script should be run before building lind-boot and glibc.
#
# Source of truth: src/glibc/lind_syscall/lind_syscall_num.h

from __future__ import annotations

import re
import sys
from pathlib import Path


def parse_c_header(header_path: str) -> dict[str, int]:
    """Parse syscall constants from glibc's lind_syscall_num.h header file."""
    syscalls = {}

    try:
        with open(header_path) as f:
            content = f.read()
    except FileNotFoundError:
        print(f"Error: Could not find {header_path}", file=sys.stderr)
        sys.exit(1)

    # Match #define SYSCALL_NAME number
    pattern = r'#define\s+(\w+)\s+(\d+)'

    for match in re.finditer(pattern, content):
        name, number = match.groups()
        syscalls[name] = int(number)

    if not syscalls:
        print(f"Error: Could not parse any syscalls from {header_path}", file=sys.stderr)
        sys.exit(1)

    return syscalls


def generate_rust_constants(syscalls: dict[str, int]) -> str:
    """Generate Rust constants for sysdefs/constants/syscall_const.rs."""
    lines = [
        "//! Syscall number constants for the Lind platform.",
        "//!",
        "//! Source of truth: Linux x86_64 syscall table",
        "//!   https://github.com/torvalds/linux/blob/v6.16-rc1/arch/x86/entry/syscalls/syscall_64.tbl",
        "//! (Historical overview: https://filippo.io/linux-syscall-table/)",
        "//!",
        "//! Keep these in sync with glibc's lind_syscall_num.h and RawPOSIX dispatcher.",
        "",
    ]

    # Sort by syscall number for readability
    sorted_syscalls = sorted(syscalls.items(), key=lambda x: x[1])

    for name, number in sorted_syscalls:
        lines.append(f"pub const {name}: i32 = {number};")

    lines.append("")
    return "\n".join(lines)





def write_file(path: str, content: str) -> None:
    """Write content to file, creating directories if needed."""
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    with open(path, 'w') as f:
        f.write(content)
    print(f"Generated: {path}")


def main() -> None:
    """Generate syscall mapping constants from glibc header."""
    # Find workspace root (should be run from lind-wasm directory)
    workspace_root = Path(__file__).parent.parent

    c_header = workspace_root / "src/glibc/lind_syscall/lind_syscall_num.h"
    rust_sysdefs_out = workspace_root / "src/sysdefs/src/constants/syscall_const.rs"

    print("Parsing syscall definitions...")
    syscalls = parse_c_header(str(c_header))
    print(f"Found {len(syscalls)} syscall definitions")

    # Generate Rust constants for sysdefs
    print("\nGenerating sysdefs constants...")
    rust_content = generate_rust_constants(syscalls)
    write_file(str(rust_sysdefs_out), rust_content)

    print("Done!")


if __name__ == "__main__":
    main()
