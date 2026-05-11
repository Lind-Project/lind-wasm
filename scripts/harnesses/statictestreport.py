#!/usr/bin/env python3
"""Static build test harness.

Runs the unit tests under tests/unit-tests/static_tests/ compiled with the
--static flag so that static WASM binaries (no dynamic linking) are tested.
This is a thin wrapper around wasmtestreport.py that pre-sets the static
compilation flags.
"""

from __future__ import annotations

import json
import subprocess
import tempfile
from pathlib import Path
from typing import Any, Callable

SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[1]
WASMTESTREPORT = SCRIPT_DIR / "wasmtestreport.py"

# Flags forwarded to wasmtestreport for static builds:
#   --run static_tests            : only run tests under static_tests/
#   --static                      : pass --static before source file in lind_compile
#   --allow-pre-compiled          : use .cwasm AOT binaries (consistent with dynamic harness)
#   --compile-flags -pthread -lpthread : link pthread for thread/TLS tests
#   --skip-libcpp                 : libc++ smoke runs in the dynamic wasm harness only (same
#                                   lind_compile_cpp path either way; avoids duplicate work/HTML)
_STATIC_HARNESS_ARGS = [
    "--run", "static_tests",
    "--static",
    "--allow-pre-compiled",
    "--compile-flags", "-pthread", "-lpthread",
    "--skip-libcpp",
]


def run_harness(
    forward_args: list[str] | None = None,
    execute_with_echo: Callable[[list[str], Path, str], tuple[int, str]] | None = None,
) -> dict[str, Any]:
    """Run static tests via wasmtestreport.py and return a harness result dict."""
    with tempfile.TemporaryDirectory(prefix="harness_statictestreport_") as tmpdir:
        tmp_path = Path(tmpdir)
        json_out = tmp_path / "static.json"
        html_out = tmp_path / "static.html"

        args = [
            "python3", str(WASMTESTREPORT),
            *_STATIC_HARNESS_ARGS,
            "--output", str(json_out),
            "--report", str(html_out),
        ]
        if forward_args:
            args.extend(forward_args)

        if execute_with_echo is not None:
            return_code, combined_output = execute_with_echo(args, REPO_ROOT, "statictestreport")
            if return_code != 0:
                raise RuntimeError(
                    "statictestreport (wasmtestreport --static) failed "
                    f"with exit code {return_code}.\nCombined output:\n{combined_output}"
                )
        else:
            proc = subprocess.run(
                args,
                capture_output=True,
                text=True,
                cwd=REPO_ROOT,
            )
            if proc.returncode != 0:
                raise RuntimeError(
                    "statictestreport (wasmtestreport --static) failed "
                    f"with exit code {proc.returncode}.\n"
                    f"STDOUT:\n{proc.stdout}\nSTDERR:\n{proc.stderr}"
                )

        report_data = json.loads(json_out.read_text(encoding="utf-8"))
        html_data = html_out.read_text(encoding="utf-8")

    return {
        "name": "static",
        "json_filename": "static.json",
        "report": report_data,
        "html": html_data,
    }
