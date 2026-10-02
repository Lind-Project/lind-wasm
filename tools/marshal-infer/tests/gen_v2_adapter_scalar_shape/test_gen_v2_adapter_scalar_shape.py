#!/usr/bin/env python3
"""Unit tests for gen_v2_adapter.py's scalar return/argument shape
confirmation (issue #22's OpenBLAS inference-to-runtime integration,
Gate 4: "build the real OpenBLAS grate").

Real compilation of Gate 2/3's own lowered OpenBLAS artifact found that
41 of 151 V2-eligible functions had a `ret: {"kind": "scalar"}` entry
with no type/size at all -- a shape V1's ABI-agnostic fpcast-emu dispatch
has always tolerated (gen_grate.py's own generic `extern long name();`
never needed accurate return metadata), but one V2 cannot: it emits an
exact, real C prototype and call, so silently defaulting an unconfirmed
shape to int32 would be right only by coincidence (true for the
index-returning functions in that batch) and WRONG -- corrupting the
real value, or failing a wasm-validator check outright -- for any
genuinely float/double-returning one. These tests cover the fix:
_v2_scalar_shape_confirmed/wasm_scalar_ctype/v2_unmarshalable_reason now
reject an unconfirmed scalar shape rather than guess one.
"""
import os
import sys
import unittest

_TOOLS_DIR = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
sys.path.insert(0, os.path.join(_TOOLS_DIR, "marshal-gen"))
import gen_v2_adapter as v2  # noqa: E402


def scalar(type_=None, size=None):
    node = {"kind": "scalar"}
    if type_ is not None:
        node["type"] = type_
    if size is not None:
        node["size"] = size
    return node


class V2ScalarShapeConfirmedTests(unittest.TestCase):
    def test_ptr_and_handle_always_confirmed(self):
        self.assertTrue(v2._v2_scalar_shape_confirmed({"kind": "ptr"}))
        self.assertTrue(v2._v2_scalar_shape_confirmed({"kind": "handle"}))

    def test_confirmed_scalar_shapes(self):
        for type_, size in (("int", 4), ("int", 8), ("float", 4), ("double", 8)):
            self.assertTrue(v2._v2_scalar_shape_confirmed(scalar(type_, size)),
                             f"({type_}, {size}) should be confirmed")

    def test_bare_scalar_with_no_type_or_size_unconfirmed(self):
        # The REAL shape found on 41 real static-inference "proven"
        # records in the OpenBLAS artifact (e.g. cblas_damax's own `ret`).
        self.assertFalse(v2._v2_scalar_shape_confirmed(scalar()))

    def test_mismatched_type_size_combo_unconfirmed(self):
        # A "double" claiming to be 4 bytes, or an "int" claiming to be
        # something other than 4/8 -- never a real shape, must not be
        # treated as confirmed just because SOME type string is present.
        self.assertFalse(v2._v2_scalar_shape_confirmed(scalar("double", 4)))
        self.assertFalse(v2._v2_scalar_shape_confirmed(scalar("float", 8)))
        self.assertFalse(v2._v2_scalar_shape_confirmed(scalar("int", 2)))


class WasmScalarCtypeTests(unittest.TestCase):
    def test_confirmed_shapes_map_correctly(self):
        self.assertEqual(v2.wasm_scalar_ctype(scalar("int", 4)), "int32_t")
        self.assertEqual(v2.wasm_scalar_ctype(scalar("int", 8)), "int64_t")
        self.assertEqual(v2.wasm_scalar_ctype(scalar("float", 4)), "float")
        self.assertEqual(v2.wasm_scalar_ctype(scalar("double", 8)), "double")

    def test_ptr_and_handle_map_to_uint32(self):
        self.assertEqual(v2.wasm_scalar_ctype({"kind": "ptr"}), "uint32_t")
        self.assertEqual(v2.wasm_scalar_ctype({"kind": "handle"}), "uint32_t")

    def test_unconfirmed_shape_raises_rather_than_defaults(self):
        # This is the exact defect found on cblas_damax's own `ret`: a
        # bare {"kind": "scalar"} must never silently become int32_t.
        with self.assertRaises(ValueError):
            v2.wasm_scalar_ctype(scalar())


class V2UnmarshalableReasonScalarShapeTests(unittest.TestCase):
    def _fn(self, ret, args=None):
        return {"name": "fake_fn", "decision": "marshal", "ret": ret, "args": args or []}

    def test_void_return_never_triggers_the_check(self):
        self.assertIsNone(v2.v2_unmarshalable_reason(self._fn({"kind": "void"})))

    def test_confirmed_scalar_return_accepted(self):
        self.assertIsNone(v2.v2_unmarshalable_reason(self._fn(scalar("double", 8))))

    def test_unconfirmed_scalar_return_rejected(self):
        r = v2.v2_unmarshalable_reason(self._fn(scalar()))
        self.assertIsNotNone(r)
        self.assertIn("no confirmed type/size", r)

    def test_handle_and_alias_returns_never_need_confirmation(self):
        for kind in ("handle", "ptr_alias_arg", "ptr_into_arg"):
            self.assertIsNone(v2.v2_unmarshalable_reason(self._fn({"kind": kind})))

    def test_unconfirmed_scalar_argument_rejected(self):
        r = v2.v2_unmarshalable_reason(self._fn({"kind": "void"}, args=[scalar()]))
        self.assertIsNotNone(r)
        self.assertIn("arg0", r)
        self.assertIn("no confirmed type/size", r)

    def test_confirmed_scalar_argument_accepted(self):
        self.assertIsNone(v2.v2_unmarshalable_reason(
            self._fn({"kind": "void"}, args=[scalar("int", 4), {"kind": "ptr"}])))


if __name__ == "__main__":
    unittest.main()
