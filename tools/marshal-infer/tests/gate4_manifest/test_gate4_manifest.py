#!/usr/bin/env python3
"""Unit tests for gate4_manifest.py's own fail-closed posture (issue
#22's OpenBLAS inference-to-runtime integration, Gate 4: "build the real
OpenBLAS grate").

verify_v2_return_against_archive.py is a pure cross-check/completion
step that never fails on its own UNRESOLVED findings (see that module's
own doc). gate4_manifest.py is where Gate 4's actual "fail closed"
decision lives: build_manifest must refuse to write a manifest or
verified marshal.json at all when ANY finding is UNRESOLVED (a real ABI
disagreement, or a marshal-decision symbol missing from the archive
entirely) -- never silently proceed with a partial, possibly
ABI-mismatched result. These tests mock verify_and_patch directly (never
needing a real archive or real llvm-nm/wasm-objdump) and use a minimal
synthetic artifact so import_openblas_inference.import_all has something
real to iterate.
"""
import json
import os
import sys
import tempfile
import unittest
from unittest import mock

_TOOLS_DIR = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
sys.path.insert(0, os.path.join(_TOOLS_DIR, "marshal-gen"))
import gate4_manifest as gm  # noqa: E402


class BuildManifestFailClosedTests(unittest.TestCase):
    def setUp(self):
        self.tmpdir = tempfile.mkdtemp()
        self.addCleanup(__import__("shutil").rmtree, self.tmpdir, ignore_errors=True)
        self.artifact_path = os.path.join(self.tmpdir, "artifact.json")
        static_record = {"name": "sfoo", "decision": "marshal", "args": [], "ret": {"kind": "void"}}
        with open(self.artifact_path, "w") as fh:
            json.dump({"functions": {"sfoo": {"source": "static", "status": "resolved",
                                               "static": static_record}}}, fh)
        self.prompts_dir = os.path.join(self.tmpdir, "prompts")
        os.makedirs(self.prompts_dir)
        self.out_manifest = os.path.join(self.tmpdir, "manifest.json")
        self.out_verified = os.path.join(self.tmpdir, "verified.marshal.json")

    def test_unresolved_finding_refuses_to_write_anything(self):
        unresolved = [{"symbol": "sfoo", "location": "ret", "claimed": "nil", "real": "f64",
                       "action": "UNRESOLVED: real wasm type is not a recognized scalar valtype"}]
        with mock.patch.object(gm.verify, "verify_and_patch",
                                return_value=([{"name": "sfoo", "decision": "marshal",
                                                 "args": [], "ret": {"kind": "void"}}], unresolved)):
            with self.assertRaises(gm.Gate4VerificationError) as ctx:
                gm.build_manifest(self.artifact_path, self.prompts_dir, "/fake/archive.a", self.out_verified)
        self.assertEqual(ctx.exception.unresolved_findings, unresolved)
        self.assertFalse(os.path.exists(self.out_verified))

    def test_missing_symbol_finding_also_refuses(self):
        unresolved = [{"symbol": "sfoo", "location": "function", "claimed": None, "real": None,
                       "action": "UNRESOLVED: symbol not found as a defined function in the real archive"}]
        with mock.patch.object(gm.verify, "verify_and_patch",
                                return_value=([{"name": "sfoo", "decision": "marshal",
                                                 "args": [], "ret": {"kind": "void"}}], unresolved)):
            with self.assertRaises(gm.Gate4VerificationError):
                gm.build_manifest(self.artifact_path, self.prompts_dir, "/fake/archive.a", self.out_verified)
        self.assertFalse(os.path.exists(self.out_verified))

    def test_no_unresolved_findings_writes_manifest_and_verified_marshal(self):
        # A real (even if empty) file is needed here -- unlike the two
        # error-path tests above, this one reaches build_manifest's own
        # archive-hashing step, which the UNRESOLVED-finding checks
        # above short-circuit before ever touching.
        fake_archive = os.path.join(self.tmpdir, "fake.a")
        open(fake_archive, "wb").close()
        with mock.patch.object(gm.verify, "verify_and_patch",
                                return_value=([{"name": "sfoo", "decision": "marshal",
                                                 "args": [], "ret": {"kind": "void"}}], [])):
            manifest = gm.build_manifest(self.artifact_path, self.prompts_dir, fake_archive, self.out_verified)
        self.assertTrue(os.path.exists(self.out_verified))
        self.assertIn("sfoo", manifest["v2_generated_symbols"])
        self.assertIsNone(manifest["runtime_verification"])


if __name__ == "__main__":
    unittest.main()
