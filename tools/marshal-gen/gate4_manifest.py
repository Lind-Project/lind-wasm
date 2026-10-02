#!/usr/bin/env python3
"""Builds Gate 4's machine-readable build manifest -- issue #22's OpenBLAS
inference-to-runtime integration, Gate 4: "build the real OpenBLAS
grate".

Produces a single, reproducible JSON document covering every fact the
plan's own Gate 4 text requires be "produced and archived": artifact and
archive identity (hash), OpenBLAS revision/configuration, per-symbol
source/status/transport/reason, and the exact generated V2/V1-fallback/
capability-rejected symbol sets -- computed directly from the same
import_openblas_inference.py/verify_v2_return_against_archive.py/
gen_grate.py/gen_v2_adapter.py functions the real build uses, not
hand-transcribed into a note afterward. build_openblas_v2_grate.sh
consumes this script's own `--out-verified-marshal` output directly for
the grate it actually compiles, so the manifest and the compiled grate
can never silently drift apart -- the single build script run produces
both, from the exact same verified marshal.json.

Usage:
  gate4_manifest.py <artifact.json> --prompts-dir <dir> --archive <lib.a> \\
      --out-manifest <manifest.json> --out-verified-marshal <verified.marshal.json>
"""
import argparse
import hashlib
import json
import os
import subprocess
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import gen_grate as gg  # noqa: E402
import gen_v2_adapter as v2  # noqa: E402
import import_openblas_inference as imp  # noqa: E402
import verify_v2_return_against_archive as verify  # noqa: E402


class Gate4VerificationError(RuntimeError):
    """Raised when verify_v2_return_against_archive.py reports one or
    more UNRESOLVED findings -- a real ABI disagreement or a marshal-
    decision symbol missing from the archive, neither of which this
    script can safely patch. Carries the exact findings so main() can
    print each one, not just a count."""
    def __init__(self, unresolved_findings):
        self.unresolved_findings = unresolved_findings
        super().__init__(f"{len(unresolved_findings)} unresolved archive-verification finding(s)")


def _sha256_of(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def _openblas_revision(archive_path):
    """Best-effort: the git commit of the checkout the archive lives in
    (lind-wasm-apps/openblas/libopenblas.a -> lind-wasm-apps's own HEAD),
    or None if the archive isn't inside a git checkout at all."""
    probe_dir = os.path.dirname(os.path.abspath(archive_path))
    try:
        out = subprocess.run(["git", "-C", probe_dir, "log", "-1", "--format=%H %s"],
                              capture_output=True, text=True, timeout=10)
        if out.returncode == 0:
            return out.stdout.strip()
    except OSError:
        pass
    return None


def build_manifest(artifact_path, prompts_dir, archive_path, out_verified_marshal):
    functions, report_rows = imp.import_all(artifact_path, prompts_dir)

    with tempfile.TemporaryDirectory() as td:
        lowered_path = os.path.join(td, "lowered.marshal.json")
        marshal_out = {
            "functions": [
                {**record, "decision": "marshal"} if record is not None
                else {"name": name, "decision": "force_local"}
                for name, record in sorted(functions.items())
            ]
        }
        with open(lowered_path, "w") as fh:
            json.dump(marshal_out, fh)
        verified_fns, findings = verify.verify_and_patch(lowered_path, archive_path)

    # verify_v2_return_against_archive.py is a pure cross-check/completion
    # step -- it never fails on its own UNRESOLVED findings (a real ABI
    # disagreement it has no safe way to repair, or a marshal-decision
    # symbol missing from the archive entirely). Gate 4's OWN "fail
    # closed" posture is enforced HERE: any unresolved finding means this
    # build is not trustworthy, so no manifest or verified marshal.json is
    # written at all -- never silently proceed with a partial, possibly
    # ABI-mismatched result.
    unresolved = [f for f in findings if f["action"].startswith("UNRESOLVED")]
    if unresolved:
        raise Gate4VerificationError(unresolved)

    with open(out_verified_marshal, "w") as fh:
        json.dump({"functions": verified_fns}, fh, indent=2)
        fh.write("\n")

    verified_by_name = {f["name"]: f for f in verified_fns}
    with open(artifact_path) as fh:
        raw = json.load(fh)["functions"]

    symbols = []
    v2_names, v1_names, rejected_names = [], [], []
    for row in report_rows:
        name = row["symbol"]
        rec = verified_by_name.get(name)
        entry = {
            "symbol": name, "source": row["source"], "status": row["status"],
            "import_reason": row["reason"], "transport": None,
        }
        if rec is not None and rec.get("decision") == "marshal":
            if v2.is_v2_marshalable(rec):
                entry["transport"] = "V2"
                v2_names.append(name)
            elif gg.is_marshalable(rec):
                entry["transport"] = "V1"
                v1_names.append(name)
            else:
                entry["reject_reason"] = gg.unmarshalable_reason(rec, max_args=None)
                rejected_names.append(name)
        symbols.append(entry)

    return {
        "artifact_path": os.path.relpath(artifact_path),
        "artifact_sha256": _sha256_of(artifact_path),
        "archive_path": archive_path,
        "archive_sha256": _sha256_of(archive_path),
        "openblas_revision": _openblas_revision(archive_path),
        "totals": {
            "total_functions": len(raw),
            "resolved": sum(1 for r in raw.values() if r.get("status") == "resolved"),
            "flagged": sum(1 for r in raw.values() if r.get("status") == "flagged"),
            "unresolved": sum(1 for r in raw.values() if r.get("status") == "unresolved"),
            "static_resolved": sum(1 for r in raw.values() if r.get("source") == "static" and r.get("status") == "resolved"),
            "llm_resolved": sum(1 for r in raw.values() if r.get("source") == "llm" and r.get("status") == "resolved"),
            "v2_generated": len(v2_names),
            "v1_fallback": len(v1_names),
            "capability_rejected": len(rejected_names),
        },
        "v2_generated_symbols": sorted(v2_names),
        "v1_fallback_symbols": sorted(v1_names),
        "capability_rejected_symbols": sorted(rejected_names),
        "archive_verification_findings": findings,
        "symbols": sorted(symbols, key=lambda e: e["symbol"]),
        # Filled in by build_openblas_v2_grate.sh after it actually
        # compiles and runs the grate -- absent here means "not yet
        # verified against a real run."
        "runtime_verification": None,
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("artifact", help="openblas-inference/final/openblas_inference.json")
    ap.add_argument("--prompts-dir", required=True)
    ap.add_argument("--archive", required=True, help="the real static archive this grate will link against")
    ap.add_argument("--out-manifest", required=True)
    ap.add_argument("--out-verified-marshal", required=True,
                     help="the verified marshal.json this manifest describes -- "
                          "feed this SAME file to gen_v2_adapter.py, never a separately regenerated one")
    args = ap.parse_args()

    try:
        manifest = build_manifest(args.artifact, args.prompts_dir, args.archive, args.out_verified_marshal)
    except Gate4VerificationError as e:
        print(f"[gate4_manifest] FATAL: {e} -- refusing to write a manifest or verified "
              f"marshal.json for an archive-ABI state this build cannot trust", file=sys.stderr)
        for f in e.unresolved_findings:
            print(f"  - {f['symbol']} {f['location']}: claimed {f['claimed']!r}, "
                  f"real archive says {f['real']!r} -- {f['action']}", file=sys.stderr)
        sys.exit(1)
    with open(args.out_manifest, "w") as fh:
        json.dump(manifest, fh, indent=2, sort_keys=True)
        fh.write("\n")

    print(f"[gate4_manifest] {manifest['totals']}")
    print(f"[gate4_manifest] wrote {args.out_manifest}")
    print(f"[gate4_manifest] wrote {args.out_verified_marshal}")


if __name__ == "__main__":
    main()
