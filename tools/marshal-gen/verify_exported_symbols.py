#!/usr/bin/env python3
"""Verifies that a compiled V2 grate's own exported `__lind_v2_adapter_*`
symbol set EXACTLY matches a Gate 4 manifest's `v2_generated_symbols` --
issue #22's OpenBLAS inference-to-runtime integration, Gate 4: "build
the real OpenBLAS grate".

A bare handler-count match (e.g. "151/151 registered") can agree by
coincidence even when the actual symbols differ -- one generated adapter
silently swapped for another of the same total count would still report
a matching count. This checks the real, compiled artifact's own export
table (read via `wasm-objdump -x`, not the AOT-precompiled `.cwasm` --
confirmed empirically that `.cwasm` is a Cranelift-compiled native
artifact wasm-objdump cannot read at all; `lind_compile`'s own "full"
mode keeps the intermediate, linked-and-opt'd `.wasm` on disk right next
to it) against the manifest's own claim, as an actual set-equality
check, so a missing OR an unexpected extra export is caught either way.

Usage:
  verify_exported_symbols.py <manifest.json> <grate.wasm>

Exit code 0 and no output change to the manifest's own
`exported_symbol_verification` field other than recording the (already-
passing) result; exit code 1, printed diagnostics, and an unmodified
manifest on a mismatch.
"""
import argparse
import json
import re
import subprocess
import sys


def exported_adapter_symbols(wasm_path):
    """The exact set of `__lind_v2_adapter_*` names this compiled
    module's own export table advertises."""
    out = subprocess.run(["wasm-objdump", "-x", wasm_path], capture_output=True, text=True, check=True).stdout
    return set(re.findall(r'-> "(__lind_v2_adapter_[^"]+)"', out))


def verify(manifest, wasm_path):
    """Returns (ok, missing, extra): `missing` is exported names the
    manifest expects but the module doesn't export; `extra` is names the
    module exports that the manifest didn't expect."""
    expected = {f"__lind_v2_adapter_{name}" for name in manifest["v2_generated_symbols"]}
    actual = exported_adapter_symbols(wasm_path)
    missing = sorted(expected - actual)
    extra = sorted(actual - expected)
    return (not missing and not extra), missing, extra


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("manifest_json", help="Gate 4 manifest (gate4_manifest.py's own output)")
    ap.add_argument("grate_wasm", help="the linked, opt'd .wasm (NOT the AOT .cwasm) lind_compile left on disk")
    args = ap.parse_args()

    with open(args.manifest_json) as fh:
        manifest = json.load(fh)
    ok, missing, extra = verify(manifest, args.grate_wasm)

    manifest["exported_symbol_verification"] = {
        "expected_count": len(manifest["v2_generated_symbols"]),
        "missing_from_export": missing, "unexpected_in_export": extra, "exact_match": ok,
    }
    with open(args.manifest_json, "w") as fh:
        json.dump(manifest, fh, indent=2, sort_keys=True)
        fh.write("\n")

    if not ok:
        print("[verify_exported_symbols] FAIL: exported adapter symbol set disagrees "
              "with the manifest's v2_generated_symbols", file=sys.stderr)
        if missing:
            print(f"  expected but not exported: {missing}", file=sys.stderr)
        if extra:
            print(f"  exported but not expected: {extra}", file=sys.stderr)
        sys.exit(1)
    print(f"[verify_exported_symbols] ok: {len(manifest['v2_generated_symbols'])} "
          f"exported adapter symbols exactly match the manifest")


if __name__ == "__main__":
    main()
