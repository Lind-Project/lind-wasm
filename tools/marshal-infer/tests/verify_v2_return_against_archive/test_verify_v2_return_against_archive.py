#!/usr/bin/env python3
"""Unit tests for verify_v2_return_against_archive.py's pure-Python logic
(issue #22's OpenBLAS inference-to-runtime integration, Gate 4: "build
the real OpenBLAS grate").

Covers _claimed_scalar_wasm_type and the completion/correction logic in
_complete_or_correct_arg/_complete_or_correct_return/verify_and_patch,
with the real llvm-nm/wasm-objdump/archive lookup (real_function_wasm_type)
replaced by a fake lookup table -- these tests never need a real .a on
disk, matching the project's established "keep synthetic-record tests
independent of real tool/data availability" convention (see e.g.
openblas_inference_import/test_import_openblas_inference.py's own module
doc). test_real_artifact-style integration coverage against the actual
libopenblas.a lives in Gate 4's own local-notes verification log, not
here, since that needs a real sibling-repo build this suite can't assume.
"""
import json
import os
import sys
import tempfile
import unittest
from unittest import mock

_TOOLS_DIR = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
sys.path.insert(0, os.path.join(_TOOLS_DIR, "marshal-gen"))
import verify_v2_return_against_archive as verify  # noqa: E402


class ClaimedScalarWasmTypeTests(unittest.TestCase):
    def test_confirmed_shapes(self):
        self.assertEqual(verify._claimed_scalar_wasm_type({"type": "double", "size": 8}), "f64")
        self.assertEqual(verify._claimed_scalar_wasm_type({"type": "float", "size": 4}), "f32")
        self.assertEqual(verify._claimed_scalar_wasm_type({"type": "int", "size": 8}), "i64")
        self.assertEqual(verify._claimed_scalar_wasm_type({"type": "int", "size": 4}), "i32")

    def test_unconfirmed_or_mismatched_shapes_return_none(self):
        self.assertIsNone(verify._claimed_scalar_wasm_type({}))
        self.assertIsNone(verify._claimed_scalar_wasm_type({"type": "double", "size": 4}))


class CompleteOrCorrectArgTests(unittest.TestCase):
    def test_ptr_arg_matching_i32_untouched(self):
        arg = {"kind": "ptr"}
        findings = []
        verify._complete_or_correct_arg(arg, "i32", "fn", 0, findings)
        self.assertEqual(findings, [])
        self.assertEqual(arg, {"kind": "ptr"})

    def test_ptr_arg_disagreeing_with_real_type_is_unresolved_not_patched(self):
        arg = {"kind": "ptr"}
        findings = []
        verify._complete_or_correct_arg(arg, "f64", "fn", 2, findings)
        self.assertEqual(len(findings), 1)
        self.assertTrue(findings[0]["action"].startswith("UNRESOLVED"))
        self.assertEqual(arg, {"kind": "ptr"})  # never rewritten

    def test_unconfirmed_scalar_arg_is_completed(self):
        arg = {"kind": "scalar"}
        findings = []
        verify._complete_or_correct_arg(arg, "f64", "fn", 1, findings)
        self.assertEqual(arg, {"kind": "scalar", "type": "double", "size": 8})
        self.assertEqual(len(findings), 1)
        self.assertIn("completed", findings[0]["action"])

    def test_disagreeing_confirmed_scalar_arg_is_corrected(self):
        arg = {"kind": "scalar", "type": "int", "size": 4}
        findings = []
        verify._complete_or_correct_arg(arg, "f32", "fn", 0, findings)
        self.assertEqual(arg, {"kind": "scalar", "type": "float", "size": 4})
        self.assertEqual(len(findings), 1)
        self.assertIn("corrected", findings[0]["action"])

    def test_matching_confirmed_scalar_arg_untouched(self):
        arg = {"kind": "scalar", "type": "int", "size": 4}
        findings = []
        verify._complete_or_correct_arg(arg, "i32", "fn", 0, findings)
        self.assertEqual(arg, {"kind": "scalar", "type": "int", "size": 4})
        self.assertEqual(findings, [])


class CompleteOrCorrectReturnTests(unittest.TestCase):
    def test_void_matching_nil_untouched(self):
        f = {"name": "fn", "ret": {"kind": "void"}}
        findings = []
        verify._complete_or_correct_return(f, "nil", findings)
        self.assertEqual(f["ret"], {"kind": "void"})
        self.assertEqual(findings, [])

    def test_void_disagreeing_with_real_scalar_is_completed(self):
        # The exact real defect: cblas_dnrm2 claims void, the real
        # archive says it returns f64.
        f = {"name": "cblas_dnrm2", "ret": {"kind": "void"}}
        findings = []
        verify._complete_or_correct_return(f, "f64", findings)
        self.assertEqual(f["ret"], {"kind": "scalar", "type": "double", "size": 8})
        self.assertEqual(len(findings), 1)
        self.assertIn("completed", findings[0]["action"])

    def test_unconfirmed_scalar_return_is_completed(self):
        # The exact real defect: cblas_damax's own `ret` is a bare
        # {"kind": "scalar"} with no type/size.
        f = {"name": "cblas_damax", "ret": {"kind": "scalar"}}
        findings = []
        verify._complete_or_correct_return(f, "f64", findings)
        self.assertEqual(f["ret"], {"kind": "scalar", "type": "double", "size": 8})
        self.assertEqual(len(findings), 1)
        self.assertIn("completed", findings[0]["action"])

    def test_disagreeing_confirmed_scalar_return_is_corrected(self):
        f = {"name": "fn", "ret": {"kind": "scalar", "type": "int", "size": 4}}
        findings = []
        verify._complete_or_correct_return(f, "f32", findings)
        self.assertEqual(f["ret"], {"kind": "scalar", "type": "float", "size": 4})
        self.assertEqual(len(findings), 1)
        self.assertIn("corrected", findings[0]["action"])

    def test_matching_confirmed_scalar_return_untouched(self):
        f = {"name": "fn", "ret": {"kind": "scalar", "type": "double", "size": 8}}
        findings = []
        verify._complete_or_correct_return(f, "f64", findings)
        self.assertEqual(f["ret"], {"kind": "scalar", "type": "double", "size": 8})
        self.assertEqual(findings, [])

    def test_handle_and_alias_returns_matching_i32_untouched(self):
        for kind in ("handle", "ptr_alias_arg", "ptr_into_arg"):
            f = {"name": "fn", "ret": {"kind": kind}}
            findings = []
            verify._complete_or_correct_return(f, "i32", findings)
            self.assertEqual(f["ret"], {"kind": kind})
            self.assertEqual(findings, [])

    def test_unsupported_return_kind_is_never_touched(self):
        # openblas_get_config/openblas_get_corename's real shape:
        # "ptr_to_static" -- a kind neither gen_grate.py's nor
        # gen_v2_adapter.py's own SUPPORTED_RET recognizes at all, a
        # completely different, pre-existing rejection. Must never be
        # mistaken for an unconfirmed scalar and "completed" into one.
        f = {"name": "openblas_get_config", "ret": {"kind": "ptr_to_static", "copyout_bytes": 0}}
        findings = []
        verify._complete_or_correct_return(f, "i32", findings)
        self.assertEqual(f["ret"], {"kind": "ptr_to_static", "copyout_bytes": 0})
        self.assertEqual(findings, [])

    def test_handle_and_alias_returns_disagreeing_are_unresolved_not_patched(self):
        # The real gen_v2_adapter.py bug this review caught: handle/
        # ptr_alias_arg/ptr_into_arg resolve to i32 (uint32_t), never
        # "nil" -- a mismatch here must be reported, never silently
        # rewritten into a scalar shape (there is no scalar shape for a
        # pointer-returning function).
        for kind in ("handle", "ptr_alias_arg", "ptr_into_arg"):
            f = {"name": "fn", "ret": {"kind": kind}}
            findings = []
            verify._complete_or_correct_return(f, "f64", findings)
            self.assertEqual(f["ret"], {"kind": kind})  # never rewritten
            self.assertEqual(len(findings), 1)
            self.assertTrue(findings[0]["action"].startswith("UNRESOLVED"))


class VerifyAndPatchTests(unittest.TestCase):
    def _run(self, functions, real_types):
        """`real_types` maps symbol -> (param_types, ret_type), the same
        shape real_function_wasm_type returns. Patches the real
        llvm-nm/wasm-objdump lookups to this fake table, so this test
        never needs an actual .a on disk."""
        with tempfile.TemporaryDirectory() as td:
            path = os.path.join(td, "test.marshal.json")
            with open(path, "w") as fh:
                json.dump({"functions": functions}, fh)
            with mock.patch.object(verify, "_symbol_to_member", return_value={}), \
                 mock.patch.object(verify, "real_function_wasm_type",
                                   side_effect=lambda archive, sym, m, wd: real_types.get(sym)):
                return verify.verify_and_patch(path, "/fake/archive.a")

    def test_end_to_end_completes_both_args_and_return(self):
        fns = [{"name": "cblas_dnrm2", "decision": "marshal", "ret": {"kind": "void"},
                "args": [{"kind": "scalar"}, {"kind": "ptr"}, {"kind": "scalar", "type": "int", "size": 4}]}]
        fns, findings = self._run(fns, {"cblas_dnrm2": (["i32", "i32", "i32"], "f64")})
        self.assertEqual(fns[0]["args"][0], {"kind": "scalar", "type": "int", "size": 4})
        self.assertEqual(fns[0]["args"][1], {"kind": "ptr"})
        self.assertEqual(fns[0]["args"][2], {"kind": "scalar", "type": "int", "size": 4})
        self.assertEqual(fns[0]["ret"], {"kind": "scalar", "type": "double", "size": 8})
        self.assertEqual(len(findings), 2)  # arg0 completed, ret completed

    def test_force_local_records_are_never_checked(self):
        fns = [{"name": "some_fn", "decision": "force_local", "ret": {"kind": "void"}}]
        fns, findings = self._run(fns, {"some_fn": ([], "f64")})
        self.assertEqual(findings, [])

    def test_symbol_not_in_archive_is_unresolved_not_silently_skipped(self):
        # A marshal-decision symbol that isn't even a defined function in
        # the archive this grate is about to link against is never a
        # harmless gap -- must be reported, never silently passed
        # through as if nothing were wrong.
        fns = [{"name": "not_in_archive", "decision": "marshal", "ret": {"kind": "void"}, "args": []}]
        fns, findings = self._run(fns, {})  # no entry -> real_function_wasm_type returns None
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0]["symbol"], "not_in_archive")
        self.assertTrue(findings[0]["action"].startswith("UNRESOLVED"))
        self.assertEqual(fns[0]["ret"], {"kind": "void"})  # never guessed at

    def test_zero_claimed_args_but_real_archive_has_params_is_unresolved(self):
        # The original implementation's own `elif args:` guard skipped
        # this case entirely (an empty claimed args list is falsy) --
        # exactly the same silent-skip bug as the missing-symbol case
        # above, just on the args side instead.
        fns = [{"name": "fn", "decision": "marshal", "ret": {"kind": "void"}, "args": []}]
        fns, findings = self._run(fns, {"fn": (["i32", "i32"], "nil")})
        unresolved = [f for f in findings if f["location"] == "args"]
        self.assertEqual(len(unresolved), 1)
        self.assertTrue(unresolved[0]["action"].startswith("UNRESOLVED"))

    def test_arg_count_mismatch_is_unresolved_and_args_untouched(self):
        fns = [{"name": "fn", "decision": "marshal", "ret": {"kind": "void"},
                "args": [{"kind": "scalar"}]}]
        fns, findings = self._run(fns, {"fn": (["i32", "i32"], "nil")})
        unresolved = [f for f in findings if f["location"] == "args"]
        self.assertEqual(len(unresolved), 1)
        self.assertTrue(unresolved[0]["action"].startswith("UNRESOLVED"))
        self.assertEqual(fns[0]["args"][0], {"kind": "scalar"})  # untouched


if __name__ == "__main__":
    unittest.main()
