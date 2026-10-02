#!/usr/bin/env python3
"""Unit tests for verify_exported_symbols.py (issue #22's OpenBLAS
inference-to-runtime integration, Gate 4: "build the real OpenBLAS
grate").

Covers verify()'s set-equality logic directly, with the real
wasm-objdump call (exported_adapter_symbols) replaced by a fake set --
never needs a real compiled .wasm on disk, matching the project's
established "keep synthetic-record tests independent of real tool/data
availability" convention.
"""
import json
import os
import sys
import tempfile
import unittest
from unittest import mock

_TOOLS_DIR = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
sys.path.insert(0, os.path.join(_TOOLS_DIR, "marshal-gen"))
import verify_exported_symbols as ves  # noqa: E402


class VerifyTests(unittest.TestCase):
    def test_exact_match(self):
        manifest = {"v2_generated_symbols": ["cblas_daxpy", "cblas_dscal"]}
        with mock.patch.object(ves, "exported_adapter_symbols",
                                return_value={"__lind_v2_adapter_cblas_daxpy",
                                              "__lind_v2_adapter_cblas_dscal"}):
            ok, missing, extra = ves.verify(manifest, "/fake/grate.wasm")
        self.assertTrue(ok)
        self.assertEqual(missing, [])
        self.assertEqual(extra, [])

    def test_missing_export_detected(self):
        # The manifest expects cblas_dscal to be generated, but the real
        # compiled module doesn't actually export it -- e.g. a generator
        # bug that silently dropped it between manifest-build time and
        # grate-generation time.
        manifest = {"v2_generated_symbols": ["cblas_daxpy", "cblas_dscal"]}
        with mock.patch.object(ves, "exported_adapter_symbols",
                                return_value={"__lind_v2_adapter_cblas_daxpy"}):
            ok, missing, extra = ves.verify(manifest, "/fake/grate.wasm")
        self.assertFalse(ok)
        self.assertEqual(missing, ["__lind_v2_adapter_cblas_dscal"])
        self.assertEqual(extra, [])

    def test_unexpected_extra_export_detected(self):
        # The real module exports something the manifest never asked
        # for -- e.g. a stale grate .wasm left over from a previous,
        # different build.
        manifest = {"v2_generated_symbols": ["cblas_daxpy"]}
        with mock.patch.object(ves, "exported_adapter_symbols",
                                return_value={"__lind_v2_adapter_cblas_daxpy",
                                              "__lind_v2_adapter_cblas_dscal"}):
            ok, missing, extra = ves.verify(manifest, "/fake/grate.wasm")
        self.assertFalse(ok)
        self.assertEqual(missing, [])
        self.assertEqual(extra, ["__lind_v2_adapter_cblas_dscal"])

    def test_same_count_but_different_symbols_is_still_caught(self):
        # The exact scenario a bare count comparison ("N/N registered")
        # cannot distinguish from a real match: one symbol silently
        # swapped for another, same total count.
        manifest = {"v2_generated_symbols": ["cblas_daxpy", "cblas_dscal"]}
        with mock.patch.object(ves, "exported_adapter_symbols",
                                return_value={"__lind_v2_adapter_cblas_daxpy",
                                              "__lind_v2_adapter_cblas_dswap"}):
            ok, missing, extra = ves.verify(manifest, "/fake/grate.wasm")
        self.assertFalse(ok)
        self.assertEqual(missing, ["__lind_v2_adapter_cblas_dscal"])
        self.assertEqual(extra, ["__lind_v2_adapter_cblas_dswap"])


class MainTests(unittest.TestCase):
    def _run_main(self, manifest, exported):
        with tempfile.TemporaryDirectory() as td:
            manifest_path = os.path.join(td, "manifest.json")
            with open(manifest_path, "w") as fh:
                json.dump(manifest, fh)
            with mock.patch.object(ves, "exported_adapter_symbols", return_value=exported), \
                 mock.patch.object(sys, "argv", ["verify_exported_symbols.py", manifest_path, "/fake/grate.wasm"]):
                try:
                    ves.main()
                    exit_code = 0
                except SystemExit as e:
                    exit_code = e.code
            with open(manifest_path) as fh:
                updated = json.load(fh)
            return exit_code, updated

    def test_matching_exits_zero_and_records_exact_match(self):
        exit_code, updated = self._run_main(
            {"v2_generated_symbols": ["cblas_daxpy"]}, {"__lind_v2_adapter_cblas_daxpy"})
        self.assertEqual(exit_code, 0)
        self.assertTrue(updated["exported_symbol_verification"]["exact_match"])

    def test_mismatch_exits_nonzero_and_records_the_disagreement(self):
        exit_code, updated = self._run_main(
            {"v2_generated_symbols": ["cblas_daxpy", "cblas_dscal"]}, {"__lind_v2_adapter_cblas_daxpy"})
        self.assertEqual(exit_code, 1)
        self.assertFalse(updated["exported_symbol_verification"]["exact_match"])
        self.assertEqual(updated["exported_symbol_verification"]["missing_from_export"],
                          ["__lind_v2_adapter_cblas_dscal"])


if __name__ == "__main__":
    unittest.main()
