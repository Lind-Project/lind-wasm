#!/usr/bin/env python3
"""Unit tests for compare_static.py's comparison logic -- see plan section 9:
exact-match, disagreement, and static-unresolved fixtures. There is
deliberately no "representationally equivalent" fixture: compare_static.py
does not implement any equivalence rule (see its own comment on why), so
that category, while defined, currently has no code path that produces it.
"""

import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import compare_static as cs


def leaf(source, argument_id=None, constant_value=None):
    """Matches compare_static.py's own _leaf: both keys always present, so a
    fixture built with this helper compares equal (==) to what
    normalize_static_ptr_arg actually returns."""
    return {"source": source, "argument_id": argument_id, "constant_value": constant_value}


STRIDE_STATIC_ARG = {
    "kind": "ptr", "dir": "in", "size_kind": "stride_vector",
    "size_operand": {"arg_index": 0, "source": "value"},
    "stride_operand": {"arg_index": 3, "source": "value"},
    "const_size": 8,
}
STRIDE_LLM_ENTRY = {
    "id": "arg2", "direction": "in", "extent": "stride_vector",
    "extent_operand": {
        "size": leaf("argument_value", argument_id="arg0"),
        "stride": leaf("argument_value", argument_id="arg3"),
    },
}


class NormalizeTests(unittest.TestCase):
    def test_const(self):
        d, e, op, unmappable = cs.normalize_static_ptr_arg(
            {"dir": "in", "size_kind": "const", "const_size": 4})
        self.assertEqual(
            (d, e, op, unmappable),
            ("in", "constant", {"size": leaf("constant", constant_value=4)}, None))

    def test_cstr(self):
        d, e, op, unmappable = cs.normalize_static_ptr_arg({"dir": "in", "size_kind": "cstr"})
        self.assertEqual((d, e, op, unmappable), ("in", "c_string", None, None))

    def test_from_arg(self):
        d, e, op, unmappable = cs.normalize_static_ptr_arg(
            {"dir": "in", "size_kind": "from_arg", "size_arg_index": 2})
        self.assertEqual(op, {"size": leaf("argument_value", argument_id="arg2")})
        self.assertEqual(e, "argument")

    def test_from_arg_pointee(self):
        d, e, op, unmappable = cs.normalize_static_ptr_arg(
            {"dir": "out", "size_kind": "from_arg_pointee", "size_arg_index": 1})
        self.assertEqual(op, {"size": leaf("argument_pointee", argument_id="arg1")})
        self.assertEqual(e, "argument")

    def test_stride_vector_value_sources(self):
        d, e, op, unmappable = cs.normalize_static_ptr_arg(STRIDE_STATIC_ARG)
        self.assertEqual(e, "stride_vector")
        self.assertEqual(op, STRIDE_LLM_ENTRY["extent_operand"])

    def test_stride_vector_pointee_and_constant_sources(self):
        static_arg = {
            "kind": "ptr", "dir": "in", "size_kind": "stride_vector",
            "size_operand": {"arg_index": 0, "source": "pointee_i32"},
            "stride_operand": {"arg_index": -1, "source": "constant", "const_value": 1},
            "const_size": 8,
        }
        d, e, op, unmappable = cs.normalize_static_ptr_arg(static_arg)
        self.assertEqual(op["size"], leaf("argument_pointee", argument_id="arg0"))
        self.assertEqual(op["stride"], leaf("constant", constant_value=1))

    def test_ptr_array_is_unmappable(self):
        d, e, op, unmappable = cs.normalize_static_ptr_arg({"dir": "in", "size_kind": "ptr_array"})
        self.assertIsNone(e)
        self.assertIsNotNone(unmappable)


class CompareArgumentTests(unittest.TestCase):
    def test_exact_agreement(self):
        cat, detail = cs.compare_argument(STRIDE_STATIC_ARG, STRIDE_LLM_ENTRY, llm_was_queried=True)
        self.assertEqual(cat, cs.CATEGORY_EXACT)

    def test_direction_disagreement(self):
        llm = dict(STRIDE_LLM_ENTRY, direction="out")
        cat, detail = cs.compare_argument(STRIDE_STATIC_ARG, llm, llm_was_queried=True)
        self.assertEqual(cat, cs.CATEGORY_DIRECTION_DISAGREE)
        self.assertEqual(detail["static_direction"], "in")
        self.assertEqual(detail["llm_direction"], "out")

    def test_extent_disagreement(self):
        llm = dict(STRIDE_LLM_ENTRY, extent="c_string", extent_operand=None)
        cat, detail = cs.compare_argument(STRIDE_STATIC_ARG, llm, llm_was_queried=True)
        self.assertEqual(cat, cs.CATEGORY_EXTENT_DISAGREE)

    def test_operand_disagreement_same_extent(self):
        llm = {
            "id": "arg2", "direction": "in", "extent": "stride_vector",
            "extent_operand": {
                "size": leaf("argument_value", argument_id="arg0"),
                "stride": leaf("constant", constant_value=1),  # static said arg3, not constant 1
            },
        }
        cat, detail = cs.compare_argument(STRIDE_STATIC_ARG, llm, llm_was_queried=True)
        self.assertEqual(cat, cs.CATEGORY_EXTENT_DISAGREE)
        self.assertIn("static_operand", detail)

    def test_static_unresolved_llm_usable(self):
        cat, detail = cs.compare_argument(None, {"id": "arg1", "direction": "in", "extent": "one"},
                                          llm_was_queried=True)
        self.assertEqual(cat, cs.CATEGORY_LLM_ONLY)

    def test_static_available_llm_unusable(self):
        cat, detail = cs.compare_argument(STRIDE_STATIC_ARG, None, llm_was_queried=True)
        self.assertEqual(cat, cs.CATEGORY_STATIC_ONLY)

    def test_neither_available_llm_never_queried(self):
        cat, detail = cs.compare_argument(None, None, llm_was_queried=False)
        self.assertEqual(cat, cs.CATEGORY_NEITHER_AVAILABLE)

    def test_neither_available_llm_queried_but_failed(self):
        # A force_local function whose LLM query also failed: static_arg is
        # None (no static coverage at all), so this must NOT be reported as
        # "static available" -- see compare_argument's own comment on the
        # bug this regression test guards against.
        cat, detail = cs.compare_argument(None, None, llm_was_queried=True)
        self.assertEqual(cat, cs.CATEGORY_NEITHER_AVAILABLE)

    def test_ptr_array_static_is_extent_disagreement_not_silently_dropped(self):
        static_arg = {"kind": "ptr", "dir": "in", "size_kind": "ptr_array"}
        llm = {"id": "arg1", "direction": "in", "extent": "one"}
        cat, detail = cs.compare_argument(static_arg, llm, llm_was_queried=True)
        self.assertEqual(cat, cs.CATEGORY_EXTENT_DISAGREE)
        self.assertIn("reason", detail)


if __name__ == "__main__":
    unittest.main()
