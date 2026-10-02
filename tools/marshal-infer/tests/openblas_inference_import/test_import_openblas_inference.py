#!/usr/bin/env python3
"""Unit tests for import_openblas_inference.py (issue #22's OpenBLAS
inference-to-runtime integration, Gate 2: "import and lower the unified
inference result").

Covers both the general lowering mechanism (synthetic records, so a test
doesn't silently stop meaning anything if the real artifact's shape
changes) and the specific real-data findings this module's own design was
built against:

- the "one"-extent name-classification table (char flag / int dimension /
  precision-typed real all share one LLM extent category);
- the "argument"-extent byte-formula check, which must prove the formula's
  TOP-LEVEL operation actually multiplies the element count by the
  function's own precision size -- not merely that the right number
  appears somewhere in the tree (a formula that only ADDS or DIVIDES by
  that number is still an element count, not a byte count);
- ABI-type checking on expression leaves: an `argument_value` leaf must
  name a real scalar argument, an `argument_pointee` leaf must name a real
  pointer argument -- the inverse of either is a silent misinterpretation
  of that argument's own raw bits, not a usable formula.
"""
import json
import os
import sys
import tempfile
import unittest

_TOOLS_DIR = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
sys.path.insert(0, os.path.join(_TOOLS_DIR, "marshal-gen"))
import import_openblas_inference as imp  # noqa: E402


def manifest(entry_arguments):
    return {"entry_arguments": entry_arguments}


def entry_args(*specs):
    """Builds a synthetic entry_arguments list. Each spec is either a bare
    llvm_type string (name defaults to "argN") or an (llvm_type, name)
    pair: entry_args("i32", ("ptr", "n")) -> [{"id":"arg0","name":"arg0",
    "llvm_type":"i32"}, {"id":"arg1","name":"n","llvm_type":"ptr"}]."""
    out = []
    for i, spec in enumerate(specs):
        llvm_type, name = spec if isinstance(spec, tuple) else (spec, f"arg{i}")
        out.append({"id": f"arg{i}", "name": name, "llvm_type": llvm_type})
    return out


def val_leaf(idx):
    return {"argument_id": f"arg{idx}", "constant_value": None, "source": "argument_value"}


def pointee_leaf(idx):
    return {"argument_id": f"arg{idx}", "constant_value": None, "source": "argument_pointee"}


def const_leaf(value):
    return {"argument_id": None, "constant_value": value, "source": "constant"}


class LowerOperandTreeTests(unittest.TestCase):
    def test_constant_leaf(self):
        self.assertEqual(imp.lower_operand_tree(const_leaf(7), entry_args(), "t"), {"op": "constant", "value": 7})

    def test_argument_value_leaf(self):
        got = imp.lower_operand_tree(val_leaf(2), entry_args("i32", "i32", "i32"), "t")
        self.assertEqual(got, {"op": "arg_value", "arg_index": 2, "leaf_type": "i32"})

    def test_argument_pointee_leaf_maps_to_arg_pointee_i32(self):
        # "n" is on the known OpenBLAS integer-dimension name profile --
        # see test_argument_pointee_referencing_unrecognized_name_rejected
        # for the case where the pointee's name ISN'T on that profile.
        got = imp.lower_operand_tree(pointee_leaf(1), entry_args("i32", ("ptr", "n")), "t")
        self.assertEqual(got, {"op": "arg_pointee_i32", "arg_index": 1})

    def test_out_of_range_arg_index_rejected(self):
        with self.assertRaises(imp.LowerError):
            imp.lower_operand_tree(val_leaf(99), entry_args("i32", "i32"), "t")

    def test_malformed_argument_id_rejected(self):
        node = {"argument_id": "not-an-arg-id", "constant_value": None, "source": "argument_value"}
        with self.assertRaises(imp.LowerError):
            imp.lower_operand_tree(node, entry_args("i32"), "t")

    def test_unknown_source_rejected(self):
        node = {"argument_id": None, "constant_value": None, "source": "mystery"}
        with self.assertRaises(imp.LowerError):
            imp.lower_operand_tree(node, entry_args(), "t")

    def test_argument_value_referencing_a_pointer_is_rejected(self):
        # arg0 is "ptr" at the real ABI level -- reading its raw bits as a
        # scalar VALUE (an address, not a count) must be refused, not
        # silently accepted.
        with self.assertRaises(imp.LowerError) as ctx:
            imp.lower_operand_tree(val_leaf(0), entry_args("ptr"), "t")
        self.assertIn("not a supported integer scalar", str(ctx.exception))

    def test_argument_value_referencing_float_or_double_is_rejected(self):
        for t in ("float", "double"):
            with self.assertRaises(imp.LowerError):
                imp.lower_operand_tree(val_leaf(0), entry_args(t), "t")

    def test_argument_pointee_referencing_a_scalar_is_rejected(self):
        # arg0 is "i32" at the real ABI level -- there is nothing to
        # dereference.
        with self.assertRaises(imp.LowerError) as ctx:
            imp.lower_operand_tree(pointee_leaf(0), entry_args("i32"), "t")
        self.assertIn("not a pointer", str(ctx.exception))

    def test_argument_pointee_referencing_unrecognized_name_rejected(self):
        # arg0 IS a real pointer here (unlike the test above), but its name
        # ("x", a data pointer) isn't on the known OpenBLAS integer-
        # dimension profile -- "ptr" alone doesn't prove the pointee is an
        # int rather than real array data, so this must still be rejected.
        with self.assertRaises(imp.LowerError) as ctx:
            imp.lower_operand_tree(pointee_leaf(0), entry_args(("ptr", "x")), "t")
        self.assertIn("integer-dimension profile", str(ctx.exception))

    def test_abs_operator(self):
        node = {"op": "abs", "operand": val_leaf(0)}
        self.assertEqual(
            imp.lower_operand_tree(node, entry_args("i32"), "t"),
            {"op": "abs", "operand": {"op": "arg_value", "arg_index": 0, "leaf_type": "i32"}},
        )

    def test_binary_operators(self):
        for llm_op, expr_op in (("max", "max"), ("product", "product"), ("add", "add")):
            node = {"op": llm_op, "operands": [val_leaf(0), val_leaf(1)]}
            got = imp.lower_operand_tree(node, entry_args("i32", "i32"), "t")
            self.assertEqual(got["op"], expr_op)
            self.assertEqual(got["lhs"], {"op": "arg_value", "arg_index": 0, "leaf_type": "i32"})
            self.assertEqual(got["rhs"], {"op": "arg_value", "arg_index": 1, "leaf_type": "i32"})

    def test_divide_lowers_to_ceil_divide(self):
        # Deliberate policy (not a bug): every currently-known "divide" in
        # the real artifact is a provably-exact packed-storage footprint --
        # see lind_marshal.h's LIND_EXPR_CEIL_DIVIDE doc.
        node = {"op": "divide", "operands": [val_leaf(0), const_leaf(2)]}
        got = imp.lower_operand_tree(node, entry_args("i32"), "t")
        self.assertEqual(got["op"], "ceil_divide")

    def test_unary_operator_missing_operand_rejected(self):
        with self.assertRaises(imp.LowerError):
            imp.lower_operand_tree({"op": "abs"}, entry_args(), "t")

    def test_binary_operator_wrong_arity_rejected(self):
        node = {"op": "add", "operands": [val_leaf(0)]}
        with self.assertRaises(imp.LowerError):
            imp.lower_operand_tree(node, entry_args("i32"), "t")

    def test_unsupported_operator_rejected(self):
        node = {"op": "subtract", "operands": []}
        with self.assertRaises(imp.LowerError):
            imp.lower_operand_tree(node, entry_args(), "t")

    def test_nested_tree_matches_packed_storage_formula(self):
        # ceil_divide(product(n, add(n, 1)), 2) -- the real formula found in
        # the artifact for packed symmetric/triangular storage.
        node = {
            "op": "divide",
            "operands": [
                {"op": "product", "operands": [val_leaf(0), {"op": "add", "operands": [val_leaf(0), const_leaf(1)]}]},
                const_leaf(2),
            ],
        }
        got = imp.lower_operand_tree(node, entry_args("i32"), "t")
        n = {"op": "arg_value", "arg_index": 0, "leaf_type": "i32"}
        self.assertEqual(got, {
            "op": "ceil_divide",
            "lhs": {"op": "product", "lhs": n, "rhs": {"op": "add", "lhs": n, "rhs": {"op": "constant", "value": 1}}},
            "rhs": {"op": "constant", "value": 2},
        })


class OneExtentClassificationTests(unittest.TestCase):
    def test_char_flag_names(self):
        for name in ("UPLO", "TransA", "diag", "ORDER", "Side"):
            self.assertEqual(imp.classify_one_extent_pointee(name, 8), ("char", 1))

    def test_int_dimension_names_ignore_precision(self):
        # This build's own INTEGER width (4 bytes, this frozen LP64
        # OpenBLAS build specifically -- see import_openblas_inference.py's
        # own doc on _INT_SCALAR_NAMES) is fixed regardless of the
        # function's own single/double precision.
        for precision in (4, 8):
            for name in ("N", "LDA", "incx", "K", "rows"):
                self.assertEqual(imp.classify_one_extent_pointee(name, precision), ("int", 4))

    def test_real_scalar_names_match_precision(self):
        self.assertEqual(imp.classify_one_extent_pointee("ALPHA", 4), ("float", 4))
        self.assertEqual(imp.classify_one_extent_pointee("ALPHA", 8), ("double", 8))
        self.assertEqual(imp.classify_one_extent_pointee("dd1", 8), ("double", 8))

    def test_unrecognized_name_rejected_not_guessed(self):
        self.assertIsNone(imp.classify_one_extent_pointee("some_mystery_param", 8))


class PrecisionBytesTests(unittest.TestCase):
    def test_cblas_prefix(self):
        self.assertEqual(imp.precision_bytes("cblas_dgemm"), 8)
        self.assertEqual(imp.precision_bytes("cblas_saxpy"), 4)

    def test_fortran_style_prefix(self):
        self.assertEqual(imp.precision_bytes("dger_"), 8)
        self.assertEqual(imp.precision_bytes("sdot_"), 4)

    def test_unrecognized_prefix_returns_none(self):
        self.assertIsNone(imp.precision_bytes("cblas_izamax"))


class LowerPointerArgumentTests(unittest.TestCase):
    def test_one_extent(self):
        pa = {"direction": "inout", "extent": "one", "extent_operand": None, "id": "arg0"}
        got = imp.lower_pointer_argument(pa, entry_args("ptr"), 8, "dd1")
        self.assertEqual(got["size_kind"], "const")
        self.assertEqual(got["const_size"], 8)
        self.assertEqual(got["pointee"], [{"kind": "scalar", "type": "double", "size": 8}])
        # "dd1" is a precision-typed REAL scalar, not an int -- never
        # safe to dereference via arg_pointee_i32.
        self.assertFalse(got["int32_pointee_proven"])

    def test_one_extent_char_flag(self):
        pa = {"direction": "in", "extent": "one", "extent_operand": None, "id": "arg0"}
        got = imp.lower_pointer_argument(pa, entry_args("ptr"), 8, "UPLO")
        self.assertEqual(got["const_size"], 1)
        self.assertFalse(got["int32_pointee_proven"])

    def test_one_extent_int_dimension_is_proven(self):
        # "N" is on the known by-reference INTEGER profile -- the ONE case
        # "one"-extent classification positively proves int32_pointee_proven.
        pa = {"direction": "in", "extent": "one", "extent_operand": None, "id": "arg0"}
        got = imp.lower_pointer_argument(pa, entry_args("ptr"), 8, "N")
        self.assertEqual(got["pointee"], [{"kind": "scalar", "type": "int", "size": 4}])
        self.assertTrue(got["int32_pointee_proven"])

    def test_constant_extent(self):
        pa = {"direction": "in", "extent": "constant",
              "extent_operand": {"size": const_leaf(40)}, "id": "arg5"}
        got = imp.lower_pointer_argument(pa, entry_args("ptr"), 8, "param")
        self.assertEqual(got["size_kind"], "const")
        self.assertEqual(got["const_size"], 40)
        # "param" isn't an int-profile name, and 40 != 4 anyway.
        self.assertFalse(got["int32_pointee_proven"])

    def test_constant_extent_int_profile_name_with_four_bytes_is_proven(self):
        # The REAL shape the importer sees when the LLM classifies a
        # by-reference int dimension under "constant" (rather than "one")
        # extent -- e.g. LDA with a literal constant_value of 4. The
        # pointee stays deliberately opaque (informational-only), but
        # int32_pointee_proven must positively confirm it's safe, proven
        # by the argument's own NAME, not merely its byte count.
        pa = {"direction": "in", "extent": "constant",
              "extent_operand": {"size": const_leaf(4)}, "id": "arg3"}
        got = imp.lower_pointer_argument(pa, entry_args("ptr"), 8, "LDA")
        self.assertEqual(got["pointee"], [{"kind": "scalar", "type": "opaque", "size": 1}])
        self.assertTrue(got["int32_pointee_proven"])

    def test_constant_extent_four_bytes_but_not_int_profile_name_is_not_proven(self):
        # A real single-precision REAL scalar ("alpha") can ALSO be
        # exactly 4 bytes under "constant" extent -- the byte count alone
        # must never be mistaken for proof of int-ness; only a name on
        # the known INTEGER profile proves it.
        pa = {"direction": "in", "extent": "constant",
              "extent_operand": {"size": const_leaf(4)}, "id": "arg3"}
        got = imp.lower_pointer_argument(pa, entry_args("ptr"), 4, "alpha")
        self.assertFalse(got["int32_pointee_proven"])

    def test_argument_extent_with_precision_multiplier_accepted(self):
        # product(4, divide(product(n, add(n,1)), 2)) -- cblas_sspr2's real
        # (correctly-formed) formula. n is arg2 (i32).
        n = val_leaf(2)
        formula = {"op": "product", "operands": [
            const_leaf(4),
            {"op": "divide", "operands": [
                {"op": "product", "operands": [n, {"op": "add", "operands": [n, const_leaf(1)]}]}, const_leaf(2)]},
        ]}
        pa = {"direction": "inout", "extent": "argument", "extent_operand": {"size": formula}, "id": "arg8"}
        got = imp.lower_pointer_argument(pa, entry_args("ptr", "ptr", "i32"), 4, "ap")
        self.assertEqual(got["size_kind"], "expr")
        self.assertIn("size_expr", got)
        # A dynamically-sized byte buffer is never a single int.
        self.assertFalse(got["int32_pointee_proven"])

    def test_argument_extent_missing_precision_multiplier_rejected(self):
        # product(n, lda) -- cblas_stbmv's real (defective) formula: no
        # top-level factor matching the function's own 4-byte (single)
        # precision at all.
        formula = {"op": "product", "operands": [val_leaf(4), val_leaf(7)]}
        pa = {"direction": "in", "extent": "argument", "extent_operand": {"size": formula}, "id": "arg6"}
        types = ["ptr"] * 8
        types[4] = types[7] = "i32"
        with self.assertRaises(imp.LowerError) as ctx:
            imp.lower_pointer_argument(pa, entry_args(*types), 4, "a")
        self.assertIn("precision size", str(ctx.exception))

    def test_argument_extent_with_additive_offset_rejected(self):
        # product(n, lda) + 4: contains the precision constant, but as an
        # ADDITIVE offset on top of an element count, not a multiplicative
        # byte conversion -- must still be rejected.
        formula = {"op": "add", "operands": [
            {"op": "product", "operands": [val_leaf(0), val_leaf(1)]}, const_leaf(4)]}
        pa = {"direction": "in", "extent": "argument", "extent_operand": {"size": formula}, "id": "arg2"}
        with self.assertRaises(imp.LowerError) as ctx:
            imp.lower_pointer_argument(pa, entry_args("i32", "i32", "ptr"), 4, "a")
        self.assertIn("precision size", str(ctx.exception))

    def test_argument_extent_with_division_instead_of_multiplication_rejected(self):
        # ceil_divide(product(n, lda), 4): DIVIDES by the precision size
        # instead of multiplying -- must still be rejected.
        formula = {"op": "divide", "operands": [
            {"op": "product", "operands": [val_leaf(0), val_leaf(1)]}, const_leaf(4)]}
        pa = {"direction": "in", "extent": "argument", "extent_operand": {"size": formula}, "id": "arg2"}
        with self.assertRaises(imp.LowerError) as ctx:
            imp.lower_pointer_argument(pa, entry_args("i32", "i32", "ptr"), 4, "a")
        self.assertIn("precision size", str(ctx.exception))

    def test_stride_vector_extent(self):
        size = val_leaf(0)
        stride = {"op": "abs", "operand": val_leaf(3)}
        pa = {"direction": "in", "extent": "stride_vector",
              "extent_operand": {"size": size, "stride": stride}, "id": "arg2"}
        types = ["ptr"] * 4
        types[0] = types[3] = "i32"
        got = imp.lower_pointer_argument(pa, entry_args(*types), 8, "x")
        self.assertEqual(got["size_kind"], "stride_vector")
        self.assertEqual(got["size_operand_expr"], {"op": "arg_value", "arg_index": 0, "leaf_type": "i32"})
        self.assertEqual(got["stride_operand_expr"]["op"], "abs")
        self.assertEqual(got["const_size"], 8)
        # An array of precision-typed elements, not a single int.
        self.assertFalse(got["int32_pointee_proven"])

    def test_unsupported_extent_rejected(self):
        pa = {"direction": "in", "extent": "mystery", "extent_operand": None, "id": "arg0"}
        with self.assertRaises(imp.LowerError):
            imp.lower_pointer_argument(pa, entry_args("ptr"), 8, "x")

    def test_unsupported_direction_rejected(self):
        pa = {"direction": "sideways", "extent": "one", "extent_operand": None, "id": "arg0"}
        with self.assertRaises(imp.LowerError):
            imp.lower_pointer_argument(pa, entry_args("ptr"), 8, "dd1")


class LowerLlmFunctionTests(unittest.TestCase):
    def test_scalar_and_pointer_mix(self):
        rec = {"source": "llm", "status": "resolved", "model": "test",
               "pointer_arguments": [
                   {"direction": "in", "extent": "stride_vector",
                    "extent_operand": {"size": val_leaf(0), "stride": val_leaf(2)}, "id": "arg1"},
               ]}
        m = manifest(entry_args("i32", "ptr", "i32"))
        got = imp.lower_llm_function("sfake_fn", rec, m)
        self.assertEqual(got["name"], "sfake_fn")
        self.assertEqual(got["decision"], "marshal")
        self.assertEqual(len(got["args"]), 3)
        self.assertEqual(got["args"][0], {"kind": "scalar", "type": "int", "size": 4})
        self.assertEqual(got["args"][1]["kind"], "ptr")
        self.assertEqual(got["args"][2], {"kind": "scalar", "type": "int", "size": 4})

    def test_uncovered_pointer_argument_rejected(self):
        rec = {"source": "llm", "status": "resolved", "model": "test", "pointer_arguments": []}
        m = manifest(entry_args("ptr"))
        with self.assertRaises(imp.LowerError):
            imp.lower_llm_function("sfake_fn", rec, m)

    def test_duplicate_pointer_argument_index_rejected(self):
        pa = {"direction": "in", "extent": "one", "extent_operand": None, "id": "arg0"}
        rec = {"source": "llm", "status": "resolved", "model": "test", "pointer_arguments": [pa, dict(pa)]}
        m = manifest(entry_args("ptr"))
        with self.assertRaises(imp.LowerError):
            imp.lower_llm_function("sfake_fn", rec, m)

    def test_pointer_arguments_entry_targeting_a_scalar_is_rejected(self):
        # arg0 is "i32" at the real ABI level, but a pointer_arguments
        # entry claims it's a constant-sized pointer -- a pointer_arguments
        # entry only describes HOW to marshal a pointer, it can't make a
        # scalar into one. Accepting this would marshal arg0's raw i32
        # VALUE as if it were a cross-cage ADDRESS.
        pa = {"direction": "in", "extent": "constant",
              "extent_operand": {"size": const_leaf(8)}, "id": "arg0"}
        rec = {"source": "llm", "status": "resolved", "model": "test", "pointer_arguments": [pa]}
        m = manifest(entry_args("i32"))
        with self.assertRaises(imp.LowerError) as ctx:
            imp.lower_llm_function("sfake_fn", rec, m)
        self.assertIn("not a pointer", str(ctx.exception))

    def test_stride_vector_referencing_wrong_type_argument_rejected(self):
        # arg2 is "ptr" here, not "i32" -- the stride operand wrongly names
        # a pointer as if it were a raw count.
        rec = {"source": "llm", "status": "resolved", "model": "test",
               "pointer_arguments": [
                   {"direction": "in", "extent": "stride_vector",
                    "extent_operand": {"size": val_leaf(0), "stride": val_leaf(2)}, "id": "arg1"},
               ]}
        m = manifest(entry_args("i32", "ptr", "ptr"))
        with self.assertRaises(imp.LowerError):
            imp.lower_llm_function("sfake_fn", rec, m)


class ImportAllTests(unittest.TestCase):
    """End-to-end tests against small, synthetic artifacts (not the real
    203-function one -- see test_real_artifact.py's own module doc for why
    that lives separately)."""

    def _run(self, artifact, manifests):
        with tempfile.TemporaryDirectory() as td:
            artifact_path = os.path.join(td, "artifact.json")
            with open(artifact_path, "w") as fh:
                json.dump({"functions": artifact}, fh)
            prompts_dir = os.path.join(td, "prompts")
            os.makedirs(prompts_dir)
            for name, m in manifests.items():
                with open(os.path.join(prompts_dir, f"{name}.prompt.json"), "w") as fh:
                    json.dump(m, fh)
            return imp.import_all(artifact_path, prompts_dir)

    def test_static_passthrough(self):
        static_record = {"name": "sfoo", "decision": "marshal", "args": [], "ret": {"kind": "void"}}
        functions, rows = self._run({"sfoo": {"source": "static", "status": "resolved", "static": static_record}}, {})
        self.assertEqual(functions["sfoo"], static_record)
        self.assertEqual(rows[0]["source"], "static")

    def test_flagged_is_local_with_reason(self):
        functions, rows = self._run({"sfoo": {"source": "llm", "status": "flagged"}}, {})
        self.assertIsNone(functions["sfoo"])
        self.assertTrue(rows[0]["reason"])

    def test_unresolved_is_local_with_reason(self):
        functions, rows = self._run(
            {"sfoo": {"source": "none", "status": "unresolved", "reason": "model_unknown"}}, {})
        self.assertIsNone(functions["sfoo"])
        self.assertEqual(rows[0]["reason"], "model_unknown")

    def test_missing_prompt_manifest_is_local_not_a_crash(self):
        functions, rows = self._run({"sfoo": {"source": "llm", "status": "resolved", "pointer_arguments": []}}, {})
        self.assertIsNone(functions["sfoo"])
        self.assertIn("prompt manifest", rows[0]["reason"])

    def test_no_function_dropped(self):
        artifact = {
            "a": {"source": "static", "status": "resolved", "static": {"name": "a", "decision": "marshal", "args": [], "ret": {}}},
            "b": {"source": "llm", "status": "flagged"},
            "c": {"source": "none", "status": "unresolved", "reason": "x"},
            "d": {"source": "llm", "status": "resolved", "pointer_arguments": []},
        }
        functions, rows = self._run(artifact, {})
        self.assertEqual(set(functions.keys()), set(artifact.keys()))
        self.assertEqual({r["symbol"] for r in rows}, set(artifact.keys()))


if __name__ == "__main__":
    unittest.main()
