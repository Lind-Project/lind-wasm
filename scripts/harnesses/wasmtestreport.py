#!/usr/bin/env python3
"""Adapter harness for the existing scripts/wasmtestreport.py entry point."""

from __future__ import annotations

import json
import subprocess
import tempfile
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[2]
WASM_REPORT_SCRIPT = REPO_ROOT / "scripts" / "wasmtestreport.py"


def run_wasmtestreport(extra_args: list[str] | None = None) -> tuple[dict[str, Any], str]:
    """Execute the wasm test reporter and return parsed JSON and HTML output."""
    args = ["python3", str(WASM_REPORT_SCRIPT)]
    if extra_args:
        args.extend(extra_args)

    with tempfile.TemporaryDirectory(prefix="wasmtestreport_") as tmpdir:
        tmp_path = Path(tmpdir)
        json_out = tmp_path / "wasm.json"
        html_out = tmp_path / "report.html"

        args.extend(["--output", str(json_out), "--report", str(html_out)])
        proc = subprocess.run(args, capture_output=True, text=True, cwd=REPO_ROOT)

        if proc.returncode != 0:
            raise RuntimeError(
                "scripts/wasmtestreport.py failed "
                f"with exit code {proc.returncode}.\nSTDOUT:\n{proc.stdout}\nSTDERR:\n{proc.stderr}"
            )

        report_data = json.loads(json_out.read_text(encoding="utf-8"))
        html_data = html_out.read_text(encoding="utf-8")

    return report_data, html_data


def run_harness(forward_args: list[str] | None = None) -> dict[str, Any]:
    """Execute this harness using the shared test-runner contract."""
    report_data, html_data = run_wasmtestreport(extra_args=forward_args)
    return {
        "name": "wasm",
        "json_filename": "wasm.json",
        "html_filename": "report.html",
        "report": report_data,
        "html": html_data,
    }
