#!/usr/bin/env python3
"""Unit tests for gen_grate.py's general lind_extent_expr tree support
(issue #22's OpenBLAS inference-to-runtime integration, Gate 3: "extend
generated handler/runtime support").

Covers the two new pieces Gate 3 adds on top of Gate 2's import_
openblas_inference.py output:

- LIND_SIZE_EXPR (a pointer's whole byte extent computed from a tree --
  "argument"-extent "argument" records) and LIND_SIZE_STRIDE_VECTOR's own
  tree-shaped size_operand_expr/stride_operand_expr fields (a stride_vector
  record whose size/stride can't be a single lind_extent_operand leaf, e.g.
  abs(incx));
- _valid_extent_expr_tree, the generation-time validator mirroring
  lind_marshal.h's own _lind_eval_extent_expr_depth bounds (depth, node
  count, leaf arg_index range, leaf_type) -- checked here, at generation
  time, in ADDITION to (not instead of) the runtime's identical checks.

Kept independent of the real OpenBLAS artifact's own shape (see
test_real_artifact.py's own precedent in the openblas_inference_import
test suite) so these never change just because the dataset grows.
"""
import os
import sys
import unittest

_TOOLS_DIR = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
sys.path.insert(0, os.path.join(_TOOLS_DIR, "marshal-gen"))
import gen_grate as gg  # noqa: E402


def leaf_arg_value(idx, leaf_type="i32"):
    return {"op": "arg_value", "arg_index": idx, "leaf_type": leaf_type}


def leaf_pointee(idx):
    return {"op": "arg_pointee_i32", "arg_index": idx}


def leaf_const(value):
    return {"op": "constant", "value": value}


def args_of(*kinds):
    """A minimal gen_grate-shaped args[] list -- the same {"kind": ...}
    dicts f.get("args", []) holds -- with one entry per `kinds` string
    ("scalar"/"ptr"/"handle"). Each entry is ABI-complete by default (a
    plain 4-byte int scalar / a pointer carrying the importer-proven
    int32_pointee_proven annotation -- exactly what an
    arg_value(leaf_type="i32") / arg_pointee_i32 leaf may safely
    reference), so a test that wants a MISMATCHED shape (wrong width, a
    float/double, an unproven pointee) builds that single arg dict
    explicitly instead of through this helper."""
    out = []
    for k in kinds:
        if k == "scalar":
            out.append({"kind": "scalar", "type": "int", "size": 4})
        elif k == "ptr":
            out.append({"kind": "ptr", "size_kind": "const", "const_size": 4,
                         "int32_pointee_proven": True})
        else:
            out.append({"kind": k})
    return out


class ValidExtentExprTreeTests(unittest.TestCase):
    def test_leaves_valid(self):
        self.assertIsNone(gg._valid_extent_expr_tree(leaf_const(7), args_of("scalar")))
        self.assertIsNone(gg._valid_extent_expr_tree(leaf_arg_value(0), args_of("scalar")))
        self.assertIsNone(gg._valid_extent_expr_tree(leaf_pointee(0), args_of("ptr")))

    def test_nested_tree_valid(self):
        # ceil_divide(product(n, add(n, 1)), 2) -- the real packed-storage
        # formula shape.
        n = leaf_arg_value(0)
        tree = {"op": "ceil_divide", "lhs": {
            "op": "product", "lhs": n, "rhs": {"op": "add", "lhs": n, "rhs": leaf_const(1)}},
            "rhs": leaf_const(2)}
        self.assertIsNone(gg._valid_extent_expr_tree(tree, args_of("scalar")))

    def test_abs_valid(self):
        tree = {"op": "abs", "operand": leaf_arg_value(0)}
        self.assertIsNone(gg._valid_extent_expr_tree(tree, args_of("scalar")))

    def test_unsupported_op_rejected(self):
        r = gg._valid_extent_expr_tree({"op": "subtract"}, args_of("scalar"))
        self.assertIsNotNone(r)
        self.assertIn("unsupported operator", r)

    def test_not_a_dict_rejected(self):
        self.assertIsNotNone(gg._valid_extent_expr_tree(None, args_of("scalar")))
        self.assertIsNotNone(gg._valid_extent_expr_tree("arg0", args_of("scalar")))

    def test_constant_with_negative_value_rejected(self):
        r = gg._valid_extent_expr_tree(leaf_const(-1), args_of("scalar"))
        self.assertIn("invalid value", r)

    def test_constant_above_int64_max_rejected(self):
        r = gg._valid_extent_expr_tree(leaf_const(gg.EXTENT_EXPR_CONST_VALUE_MAX + 1), args_of("scalar"))
        self.assertIn("invalid value", r)

    def test_arg_index_out_of_range_rejected_with_real_args(self):
        r = gg._valid_extent_expr_tree(leaf_arg_value(5), args_of("scalar", "scalar", "scalar"))
        self.assertIn("out of range", r)

    def test_arg_index_negative_rejected_even_with_args_none(self):
        # args=None only skips the UPPER bound and the ABI-kind check -- a
        # negative index is always malformed, the same way arg_spec_body's
        # own backstop call (which never has the real args list) must
        # still catch it.
        r = gg._valid_extent_expr_tree(leaf_arg_value(-1), None)
        self.assertIn("out of range", r)

    def test_arg_index_out_of_range_not_checked_when_args_none(self):
        # arg_spec_body's own defense-in-depth backstop call: it doesn't
        # have the real args list, so it can't range-check the upper bound
        # or the ABI kind -- unmarshalable_reason() (which always has the
        # real args list) is the actual gate for both, the same split
        # _valid_extent_operand already has.
        self.assertIsNone(gg._valid_extent_expr_tree(leaf_arg_value(999), None))

    def test_arg_value_referencing_a_pointer_is_rejected(self):
        # arg0 is "ptr" here -- reading its raw bits as a scalar VALUE (an
        # address, not a count) must be refused, matching the inverse rule
        # import_openblas_inference.py's own lower_operand_tree already
        # enforces on its side of this same boundary. The generator must
        # enforce this independently: nothing stops a different or
        # hand-edited marshal.json from reaching this code without ever
        # passing through that importer.
        r = gg._valid_extent_expr_tree(leaf_arg_value(0), args_of("ptr"))
        self.assertIsNotNone(r)
        self.assertIn("not scalar", r)

    def test_arg_pointee_referencing_a_scalar_is_rejected(self):
        # arg0 is "scalar" here -- there is nothing to dereference.
        r = gg._valid_extent_expr_tree(leaf_pointee(0), args_of("scalar"))
        self.assertIsNotNone(r)
        self.assertIn("not ptr", r)

    def test_arg_value_referencing_an_eight_byte_integer_scalar_is_rejected(self):
        # arg0 is a real, plain integer -- just 8 bytes wide, not 4. A
        # leaf declared leaf_type="i32" reading it would silently read
        # (or, depending on real endianness, misread) only half the real
        # value -- kind alone ("scalar") isn't enough to prove this leaf
        # safe, width must match too.
        wide_int_arg = [{"kind": "scalar", "type": "int", "size": 8}]
        r = gg._valid_extent_expr_tree(leaf_arg_value(0, "i32"), wide_int_arg)
        self.assertIsNotNone(r)
        self.assertIn("does not match leaf_type", r)

    def test_arg_value_referencing_a_double_scalar_is_rejected(self):
        # arg0 is a real double -- its raw bits are not an integer count
        # in any width; reading them as an i32 value would grab an
        # unrelated reinterpretation of the real number's own bits.
        double_arg = [{"kind": "scalar", "type": "double", "size": 8}]
        r = gg._valid_extent_expr_tree(leaf_arg_value(0, "i32"), double_arg)
        self.assertIsNotNone(r)
        self.assertIn("does not match leaf_type", r)

    def test_arg_pointee_referencing_a_four_byte_float_pointer_is_rejected(self):
        # arg0 is a real `float *` -- exactly 4 bytes per element, same as
        # a real int, but NOT one. A fixed 4-byte const_size alone (even
        # with no explicit int32_pointee_proven annotation at all, the
        # same as a hand-edited or non-OpenBLAS marshal.json would have)
        # must not be treated as proof of int-ness -- that implication is
        # unsound regardless of what the pointee's own descriptive type
        # label happens to say.
        float_ptr_arg = [{"kind": "ptr", "size_kind": "const", "const_size": 4,
                           "pointee": [{"kind": "scalar", "type": "float", "size": 4}]}]
        r = gg._valid_extent_expr_tree(leaf_pointee(0), float_ptr_arg)
        self.assertIsNotNone(r)
        self.assertIn("int32_pointee_proven", r)

    def test_arg_pointee_referencing_a_double_pointer_is_rejected(self):
        # arg0 is a real `double *` -- an 8-byte-per-element buffer, not a
        # single 4-byte int, and (like the float* case above) carries no
        # int32_pointee_proven annotation either.
        double_ptr_arg = [{"kind": "ptr", "size_kind": "const", "const_size": 8,
                            "pointee": [{"kind": "scalar", "type": "double", "size": 8}]}]
        r = gg._valid_extent_expr_tree(leaf_pointee(0), double_ptr_arg)
        self.assertIsNotNone(r)
        self.assertIn("int32_pointee_proven", r)

    def test_arg_pointee_referencing_an_opaque_constant_four_byte_buffer_is_accepted(self):
        # The REAL shape import_openblas_inference.py emits for a
        # by-reference int dimension classified under the LLM's
        # "constant" (rather than "one") extent: pointee[0]["type"] is
        # deliberately "opaque" (the real per-element type is
        # uninformative there), but the importer has independently PROVEN
        # (via its own name-profile check) that this is safe and recorded
        # that proof explicitly -- must NOT be rejected just because the
        # pointee's own type label isn't "int".
        opaque_const4_arg = [{"kind": "ptr", "size_kind": "const", "const_size": 4,
                               "pointee": [{"kind": "scalar", "type": "opaque", "size": 1}],
                               "int32_pointee_proven": True}]
        self.assertIsNone(gg._valid_extent_expr_tree(leaf_pointee(0), opaque_const4_arg))

    def test_arg_value_missing_leaf_type_rejected(self):
        bad = {"op": "arg_value", "arg_index": 0}
        r = gg._valid_extent_expr_tree(bad, args_of("scalar"))
        self.assertIn("leaf_type", r)

    def test_arg_value_unsigned_leaf_type_against_plain_int_scalar_rejected(self):
        # arg0 is a plain 4-byte "int" -- gen_grate's own scalar schema has
        # no explicit unsigned type string distinct from "int", so a u32
        # leaf_type must be rejected even though the WIDTH matches: nothing
        # in the schema positively asserts the argument is actually
        # unsigned, and silently accepting it would treat a sign-matching
        # guess as proven fact.
        for leaf_type in ("u32", "u64"):
            r = gg._valid_extent_expr_tree(leaf_arg_value(0, leaf_type), args_of("scalar"))
            self.assertIsNotNone(r, f"leaf_type={leaf_type!r} should have been rejected")
            self.assertIn("does not match leaf_type", r)

    def test_arg_pointee_does_not_require_leaf_type(self):
        self.assertIsNone(gg._valid_extent_expr_tree(leaf_pointee(0), args_of("ptr")))

    def test_abs_missing_operand_rejected(self):
        r = gg._valid_extent_expr_tree({"op": "abs"}, args_of("scalar"))
        self.assertIn("missing its operand", r)

    def test_binary_missing_lhs_or_rhs_rejected(self):
        for node in (
            {"op": "add", "rhs": leaf_const(1)},
            {"op": "add", "lhs": leaf_const(1)},
        ):
            r = gg._valid_extent_expr_tree(node, args_of("scalar"))
            self.assertIn("missing lhs/rhs", r)

    def test_malformed_child_propagates_up(self):
        tree = {"op": "abs", "operand": {"op": "mystery"}}
        r = gg._valid_extent_expr_tree(tree, args_of("scalar"))
        self.assertIn("unsupported operator", r)

    def test_excessive_depth_rejected(self):
        # A chain of (LIND_EXTENT_EXPR_MAX_DEPTH + 2) nested abs() nodes --
        # deeper than the shared ceiling both generation and the runtime
        # enforce.
        tree = leaf_arg_value(0)
        for _ in range(gg.LIND_EXTENT_EXPR_MAX_DEPTH + 2):
            tree = {"op": "abs", "operand": tree}
        r = gg._valid_extent_expr_tree(tree, args_of("scalar"))
        self.assertIn("exceeds maximum depth", r)

    def test_excessive_node_count_rejected(self):
        # A balanced binary tree of constant leaves, wide rather than deep,
        # exceeding LIND_EXTENT_EXPR_MAX_NODES in total node count while
        # staying within the depth ceiling -- the same "wide, not deep"
        # shape lind_marshal.h's own node-budget doc warns about.
        leaves = [leaf_const(1) for _ in range(gg.LIND_EXTENT_EXPR_MAX_NODES + 1)]
        while len(leaves) > 1:
            leaves = [{"op": "add", "lhs": leaves[i], "rhs": leaves[i + 1]}
                      for i in range(0, len(leaves) - 1, 2)] + leaves[len(leaves) // 2 * 2:]
        r = gg._valid_extent_expr_tree(leaves[0], args_of("scalar"))
        self.assertIn("exceeds maximum node count", r)


class EmitExtentExprTests(unittest.TestCase):
    def test_constant_leaf_emission(self):
        em = gg.Emitter()
        name = em.emit_extent_expr(leaf_const(4))
        self.assertEqual(len(em.decls), 1)
        self.assertIn(f"struct lind_extent_expr {name}", em.decls[0])
        self.assertIn(".kind = LIND_EXPR_CONSTANT", em.decls[0])
        self.assertIn(".const_value = 4ULL", em.decls[0])

    def test_arg_value_leaf_emission(self):
        em = gg.Emitter()
        name = em.emit_extent_expr(leaf_arg_value(2, "u32"))
        self.assertIn(".kind = LIND_EXPR_ARG_VALUE", em.decls[0])
        self.assertIn(".arg_index = 2", em.decls[0])
        self.assertIn(".leaf_type = LIND_EXTENT_LEAF_U32", em.decls[0])

    def test_arg_pointee_leaf_emission(self):
        em = gg.Emitter()
        em.emit_extent_expr(leaf_pointee(1))
        self.assertIn(".kind = LIND_EXPR_ARG_POINTEE_I32", em.decls[0])
        self.assertIn(".arg_index = 1", em.decls[0])
        self.assertNotIn("leaf_type", em.decls[0])

    def test_nested_tree_declares_children_before_parent(self):
        em = gg.Emitter()
        tree = {"op": "product", "lhs": leaf_const(4), "rhs": leaf_arg_value(0)}
        top_name = em.emit_extent_expr(tree)
        # Exactly 3 nodes (product + its two leaves); the top-level node's
        # own decl must be LAST (C requires a referenced identifier to
        # already be declared at the point its address is taken).
        self.assertEqual(len(em.decls), 3)
        self.assertIn(f"struct lind_extent_expr {top_name} = ", em.decls[-1])
        self.assertIn("LIND_EXPR_PRODUCT", em.decls[-1])
        # Both children's names appear, referenced by the parent via &name.
        lhs_name = em.decls[0].split()[4]
        rhs_name = em.decls[1].split()[4]
        self.assertIn(f"&{lhs_name}", em.decls[-1])
        self.assertIn(f"&{rhs_name}", em.decls[-1])

    def test_abs_emission_references_single_child(self):
        em = gg.Emitter()
        tree = {"op": "abs", "operand": leaf_arg_value(0)}
        em.emit_extent_expr(tree)
        self.assertIn("LIND_EXPR_ABS", em.decls[-1])
        self.assertIn(".lhs = &", em.decls[-1])
        self.assertNotIn(".rhs", em.decls[-1])


class ArgSpecBodyExprTests(unittest.TestCase):
    def test_expr_size_kind_emits_pointer_to_tree(self):
        em = gg.Emitter()
        a = {"kind": "ptr", "dir": "in", "size_kind": "expr",
             "size_expr": {"op": "product", "lhs": leaf_const(8), "rhs": leaf_arg_value(0)}}
        body = em.arg_spec_body(a)
        self.assertIn(".size_kind = LIND_SIZE_EXPR", body)
        self.assertIn(".size_expr = &", body)
        self.assertEqual(len(em.decls), 3)  # product + 2 leaves

    def test_expr_size_kind_missing_tree_raises(self):
        em = gg.Emitter()
        a = {"kind": "ptr", "dir": "in", "size_kind": "expr"}
        with self.assertRaises(ValueError):
            em.arg_spec_body(a)

    def test_expr_size_kind_malformed_tree_raises(self):
        em = gg.Emitter()
        a = {"kind": "ptr", "dir": "in", "size_kind": "expr", "size_expr": {"op": "subtract"}}
        with self.assertRaises(ValueError):
            em.arg_spec_body(a)

    def test_stride_vector_with_expr_operands_emits_both_pointers(self):
        em = gg.Emitter()
        a = {"kind": "ptr", "dir": "inout", "size_kind": "stride_vector", "const_size": 8,
             "size_operand_expr": leaf_arg_value(0),
             "stride_operand_expr": {"op": "abs", "operand": leaf_arg_value(1)}}
        body = em.arg_spec_body(a)
        self.assertIn(".size_kind = LIND_SIZE_STRIDE_VECTOR", body)
        self.assertIn(".size_operand_expr = &", body)
        self.assertIn(".stride_operand_expr = &", body)
        self.assertIn(".const_size = 8", body)
        self.assertNotIn(".size_operand =", body)
        self.assertNotIn(".stride_operand =", body)

    def test_stride_vector_with_only_one_expr_operand_raises(self):
        em = gg.Emitter()
        a = {"kind": "ptr", "dir": "in", "size_kind": "stride_vector", "const_size": 8,
             "size_operand_expr": leaf_arg_value(0)}
        with self.assertRaises(ValueError) as ctx:
            em.arg_spec_body(a)
        self.assertIn("both or neither", str(ctx.exception))

    def test_stride_vector_legacy_operands_unaffected(self):
        # No *_expr keys present at all: the pre-Gate-3 plain
        # lind_extent_operand path must still emit exactly as before.
        em = gg.Emitter()
        a = {"kind": "ptr", "dir": "in", "size_kind": "stride_vector", "const_size": 8,
             "size_operand": {"source": "value", "arg_index": 0},
             "stride_operand": {"source": "constant", "const_value": 1}}
        body = em.arg_spec_body(a)
        self.assertIn(".size_operand = {", body)
        self.assertIn(".stride_operand = {", body)
        self.assertNotIn("size_operand_expr", body)


class UnmarshalableReasonExprTests(unittest.TestCase):
    def _fn(self, ptr_arg):
        return {
            "name": "fake_fn", "decision": "marshal",
            "args": [{"kind": "scalar", "type": "int", "size": 4}, ptr_arg],
            "ret": {"kind": "void"},
        }

    def test_expr_size_kind_accepted(self):
        a = {"kind": "ptr", "dir": "in", "size_kind": "expr",
             "size_expr": {"op": "product", "lhs": leaf_const(8), "rhs": leaf_arg_value(0)},
             "pointee": [{"kind": "scalar", "type": "double", "size": 8}]}
        self.assertIsNone(gg.unmarshalable_reason(self._fn(a)))

    def test_expr_size_kind_out_of_range_index_rejected(self):
        a = {"kind": "ptr", "dir": "in", "size_kind": "expr",
             "size_expr": {"op": "product", "lhs": leaf_const(8), "rhs": leaf_arg_value(5)},
             "pointee": [{"kind": "scalar", "type": "double", "size": 8}]}
        r = gg.unmarshalable_reason(self._fn(a))
        self.assertIsNotNone(r)
        self.assertIn("size_expr", r)
        self.assertIn("out of range", r)

    def test_stride_vector_expr_operands_accepted(self):
        a = {"kind": "ptr", "dir": "in", "size_kind": "stride_vector", "const_size": 8,
             "size_operand_expr": leaf_arg_value(0),
             "stride_operand_expr": {"op": "abs", "operand": leaf_arg_value(0)},
             "pointee": [{"kind": "scalar", "type": "double", "size": 8}]}
        self.assertIsNone(gg.unmarshalable_reason(self._fn(a)))

    def _struct_arg(self, field):
        # A top-level ptr-to-struct argument whose one field is `field` --
        # the same shape emit_layout/arg_spec_body's own struct branch
        # consumes (offset/spec/touched per lind_field).
        struct_node = {"kind": "struct", "size": 8, "fields": [field]}
        return {"kind": "ptr", "dir": "in", "size_kind": "const", "const_size": 8,
                "pointee": [struct_node]}

    def test_nested_struct_field_with_expr_size_kind_rejected(self):
        # The nested-struct-field runtime switch (lind_marshal.h) has no
        # LIND_SIZE_EXPR case at all -- a generated handler for this shape
        # would compile fine and only abort on its first real call.
        field = {"kind": "ptr", "dir": "in", "size_kind": "expr",
                 "size_expr": leaf_const(8), "offset": 0, "touched": 1,
                 "pointee": [{"kind": "scalar", "type": "opaque", "size": 1}]}
        r = gg.unmarshalable_reason(self._fn(self._struct_arg(field)))
        self.assertIsNotNone(r)
        self.assertIn("nested pointer field", r)

    def test_nested_struct_field_with_stride_vector_expr_operands_rejected(self):
        # Same reasoning, for stride_vector's tree-shaped operand pair.
        field = {"kind": "ptr", "dir": "in", "size_kind": "stride_vector", "const_size": 8,
                 "size_operand_expr": leaf_const(1),
                 "stride_operand_expr": leaf_const(1),
                 "offset": 0, "touched": 1,
                 "pointee": [{"kind": "scalar", "type": "opaque", "size": 1}]}
        r = gg.unmarshalable_reason(self._fn(self._struct_arg(field)))
        self.assertIsNotNone(r)
        self.assertIn("nested pointer field", r)

    def test_stride_vector_one_sided_expr_operands_rejected(self):
        a = {"kind": "ptr", "dir": "in", "size_kind": "stride_vector", "const_size": 8,
             "size_operand_expr": leaf_arg_value(0),
             "pointee": [{"kind": "scalar", "type": "double", "size": 8}]}
        r = gg.unmarshalable_reason(self._fn(a))
        self.assertIsNotNone(r)
        self.assertIn("both or neither", r)


if __name__ == "__main__":
    unittest.main()
