#!/usr/bin/env python3
"""Unit tests for validator.py -- see plan section 9's fixture checklist:
valid results and each malformed/unknown case (duplicate/missing arguments,
scalar IDs, wrong operand sources, negative/overflow constants, extra
prose, code fences, version/function mismatches).
"""

import json
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import validator as v

MANIFEST = {
    "entry_arguments": [
        {"id": "arg0", "name": "n", "llvm_type": "i32"},
        {"id": "arg1", "name": "alpha", "llvm_type": "double"},
        {"id": "arg2", "name": "x", "llvm_type": "ptr"},
        {"id": "arg3", "name": "incx", "llvm_type": "i32"},
        {"id": "arg4", "name": "y", "llvm_type": "ptr"},
    ]
}


def leaf(source, argument_id=None, constant_value=None):
    """Builds a leaf operand carrying BOTH keys explicitly, matching the
    strict-mode contract (provider.py's RESPONSE_SCHEMA_JSON_SCHEMA) every
    fixture in this file is meant to exercise -- a leaf built by hand that
    omits the unused key would test a shape a real strict-mode response can
    never actually produce."""
    return {"source": source, "argument_id": argument_id, "constant_value": constant_value}


def resp(**overrides):
    base = {
        "response_schema_version": "marshal-response-v6",
        "function": "f",
        "pointer_arguments": [
            {"id": "arg2", "direction": "in",
             "extent_operand": {"size": leaf("argument_value", argument_id="arg0"),
                                "stride": leaf("argument_value", argument_id="arg3")},
             "extent": "stride_vector"},
            {"id": "arg4", "direction": "out",
             "extent_operand": {"size": leaf("argument_value", argument_id="arg0"),
                                "stride": leaf("argument_value", argument_id="arg3")},
             "extent": "stride_vector"},
        ],
    }
    base.update(overrides)
    return json.dumps(base)


class ValidatorTests(unittest.TestCase):
    def test_valid_usable(self):
        r = v.validate_response(resp(), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_USABLE)
        self.assertIsNotNone(r.normalized)
        self.assertEqual([e["id"] for e in r.normalized["pointer_arguments"]], ["arg2", "arg4"])

    def test_json_invalid_not_json(self):
        r = v.validate_response("not json at all", "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_JSON_INVALID)

    def test_json_invalid_code_fence(self):
        text = "```json\n" + resp() + "\n```"
        r = v.validate_response(text, "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_JSON_INVALID)

    def test_json_invalid_extra_prose(self):
        text = "Here is the answer:\n" + resp()
        r = v.validate_response(text, "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_JSON_INVALID)

    def test_json_invalid_array_not_object(self):
        r = v.validate_response("[1, 2, 3]", "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_JSON_INVALID)

    def test_schema_invalid_wrong_schema_version(self):
        r = v.validate_response(resp(response_schema_version="marshal-response-v2"), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)

    def test_schema_invalid_wrong_function(self):
        r = v.validate_response(resp(function="other_fn"), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)

    def test_schema_invalid_missing_argument(self):
        body = json.loads(resp())
        body["pointer_arguments"] = [body["pointer_arguments"][0]]  # drop arg4
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)
        self.assertTrue(any("missing" in e for e in r.errors))

    def test_schema_invalid_duplicate_id(self):
        body = json.loads(resp())
        body["pointer_arguments"].append(dict(body["pointer_arguments"][0]))
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)
        self.assertTrue(any("duplicate" in e for e in r.errors))

    def test_schema_invalid_scalar_id_classified(self):
        body = json.loads(resp())
        body["pointer_arguments"].append({"id": "arg0", "direction": "in", "extent": "one"})
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)

    def test_schema_invalid_nonexistent_id(self):
        body = json.loads(resp())
        body["pointer_arguments"].append({"id": "arg99", "direction": "in", "extent": "one"})
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)

    def test_schema_invalid_bad_direction_enum(self):
        body = json.loads(resp())
        body["pointer_arguments"][0]["direction"] = "sideways"
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)

    def test_schema_invalid_bad_extent_enum(self):
        body = json.loads(resp())
        body["pointer_arguments"][0]["extent"] = "planar"
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)

    def test_schema_invalid_wrong_operand_source(self):
        body = json.loads(resp())
        body["pointer_arguments"][0]["extent_operand"]["size"]["source"] = "bogus_source"
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)

    def test_schema_invalid_argument_value_points_at_pointer(self):
        body = json.loads(resp())
        body["pointer_arguments"][0]["extent_operand"]["size"] = leaf("argument_value", argument_id="arg2")
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)
        self.assertTrue(any("argument_value must reference a scalar" in e for e in r.errors))

    def test_schema_invalid_argument_pointee_points_at_scalar(self):
        body = json.loads(resp())
        body["pointer_arguments"][0]["extent_operand"]["size"] = leaf("argument_pointee", argument_id="arg0")
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)
        self.assertTrue(any("argument_pointee must reference a pointer" in e for e in r.errors))

    def test_schema_invalid_leaf_missing_constant_value_key(self):
        # A strict-mode response always carries both keys; a leaf missing
        # one entirely (not merely null) is a real contract violation.
        body = json.loads(resp())
        body["pointer_arguments"][0]["extent_operand"]["size"] = {"source": "argument_value", "argument_id": "arg0"}
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)
        self.assertTrue(any("must carry both argument_id and constant_value" in e for e in r.errors))

    def test_schema_invalid_negative_constant(self):
        body = json.loads(resp())
        body["pointer_arguments"][0] = {"id": "arg2", "direction": "in", "extent": "constant",
                                        "extent_operand": {"size": leaf("constant", constant_value=-1)}}
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)

    def test_schema_invalid_overflow_constant(self):
        body = json.loads(resp())
        body["pointer_arguments"][0] = {"id": "arg2", "direction": "in", "extent": "constant",
                                        "extent_operand": {"size": leaf("constant", constant_value=2**65)}}
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)

    def test_schema_invalid_bool_constant(self):
        body = json.loads(resp())
        body["pointer_arguments"][0] = {"id": "arg2", "direction": "in", "extent": "constant",
                                        "extent_operand": {"size": leaf("constant", constant_value=True)}}
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)

    def test_schema_invalid_extent_operand_present_when_forbidden(self):
        body = json.loads(resp())
        body["pointer_arguments"][0] = {"id": "arg2", "direction": "in", "extent": "one",
                                        "extent_operand": leaf("constant", constant_value=1)}
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)

    def test_schema_invalid_stride_vector_missing_stride(self):
        body = json.loads(resp())
        del body["pointer_arguments"][0]["extent_operand"]["stride"]
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)

    def test_model_unknown_direction(self):
        body = json.loads(resp())
        body["pointer_arguments"][0] = {"id": "arg2", "direction": "unknown", "extent": "unknown"}
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_MODEL_UNKNOWN)
        self.assertIsNotNone(r.normalized)

    def test_valid_constant_extent(self):
        body = json.loads(resp())
        body["pointer_arguments"][0] = {"id": "arg2", "direction": "in", "extent": "constant",
                                        "extent_operand": {"size": leaf("constant", constant_value=64)}}
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_USABLE)

    def test_valid_one_extent_no_operand(self):
        body = json.loads(resp())
        body["pointer_arguments"][0] = {"id": "arg2", "direction": "inout", "extent": "one"}
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_USABLE)

    def test_valid_leaf_with_explicit_null_unused_key(self):
        # A strict-mode JSON schema response includes BOTH argument_id and
        # constant_value on every leaf, with the unused one explicit null
        # rather than omitted -- this must validate identically to the key
        # being absent entirely.
        body = json.loads(resp())
        body["pointer_arguments"][0] = {"id": "arg2", "direction": "in", "extent": "constant",
                                        "extent_operand": {"size": {
                                            "source": "constant", "constant_value": 64, "argument_id": None,
                                        }}}
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_USABLE)

    def test_valid_c_string_no_operand(self):
        body = json.loads(resp())
        body["pointer_arguments"][0] = {"id": "arg2", "direction": "in", "extent": "c_string"}
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_USABLE)

    # ---- product / abs / max composite operands ----

    def test_valid_max_operand(self):
        # e.g. a matrix bound of lda * max(m, k) -- an "order"/"trans" flag
        # selects which of two named arguments is the true other dimension.
        body = json.loads(resp())
        body["pointer_arguments"][0] = {
            "id": "arg2", "direction": "in", "extent": "argument",
            "extent_operand": {"size": {"op": "product", "operands": [
                leaf("argument_value", argument_id="arg0"),
                {"op": "max", "operands": [
                    leaf("argument_value", argument_id="arg3"),
                    leaf("constant", constant_value=8),
                ]},
            ]}},
        }
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_USABLE)

    def test_schema_invalid_max_wrong_operand_count(self):
        body = json.loads(resp())
        body["pointer_arguments"][0] = {
            "id": "arg2", "direction": "in", "extent": "argument",
            "extent_operand": {"size": {"op": "max", "operands": [
                leaf("argument_value", argument_id="arg0"),
            ]}},
        }
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)

    def test_schema_invalid_max_with_operand_singular(self):
        # "max" takes "operands" (plural, like "product"), never "operand"
        # (singular, "abs"'s own shape) -- the two composite shapes must not
        # be interchangeable.
        body = json.loads(resp())
        body["pointer_arguments"][0]["extent_operand"]["stride"] = {
            "op": "max", "operand": leaf("argument_value", argument_id="arg3"),
        }
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)

    def test_valid_packed_triangular_add_and_divide(self):
        # The classic BLAS packed-triangular element count N*(N+1)/2:
        # divide(product(N, add(N, 1)), 2) -- CEILING division, always
        # exact here since N*(N+1) is always even.
        body = json.loads(resp())
        body["pointer_arguments"][0] = {
            "id": "arg2", "direction": "in", "extent": "argument",
            "extent_operand": {"size": {"op": "divide", "operands": [
                {"op": "product", "operands": [
                    leaf("argument_value", argument_id="arg0"),
                    {"op": "add", "operands": [
                        leaf("argument_value", argument_id="arg0"),
                        leaf("constant", constant_value=1),
                    ]},
                ]},
                leaf("constant", constant_value=2),
            ]}},
        }
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_USABLE)

    def test_schema_invalid_divide_by_literal_zero(self):
        body = json.loads(resp())
        body["pointer_arguments"][0] = {
            "id": "arg2", "direction": "in", "extent": "argument",
            "extent_operand": {"size": {"op": "divide", "operands": [
                leaf("argument_value", argument_id="arg0"),
                leaf("constant", constant_value=0),
            ]}},
        }
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)
        self.assertTrue(any("literal zero divisor" in e for e in r.errors))

    def test_schema_invalid_add_wrong_operand_count(self):
        body = json.loads(resp())
        body["pointer_arguments"][0] = {
            "id": "arg2", "direction": "in", "extent": "argument",
            "extent_operand": {"size": {"op": "add", "operands": [
                leaf("argument_value", argument_id="arg0"),
            ]}},
        }
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)

    def test_schema_invalid_divide_with_operand_singular(self):
        # "divide" takes "operands" (plural, like "product"/"add"/"max"),
        # never "operand" (singular, "abs"'s own shape).
        body = json.loads(resp())
        body["pointer_arguments"][0]["extent_operand"]["stride"] = {
            "op": "divide", "operand": leaf("argument_value", argument_id="arg3"),
        }
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)

    def test_valid_product_operand(self):
        # e.g. a matrix bound of lda * rows -- "argument" extent, whose
        # extent_operand IS a product of two independent scalar arguments.
        body = json.loads(resp())
        body["pointer_arguments"][0] = {
            "id": "arg2", "direction": "in", "extent": "argument",
            "extent_operand": {"size": {"op": "product", "operands": [
                leaf("argument_value", argument_id="arg0"),
                leaf("argument_value", argument_id="arg3"),
            ]}},
        }
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_USABLE)

    def test_valid_abs_operand(self):
        # e.g. daxpy_-style stride_vector: stride = abs(incx).
        body = json.loads(resp())
        body["pointer_arguments"][0]["extent_operand"]["stride"] = {
            "op": "abs", "operand": leaf("argument_value", argument_id="arg3"),
        }
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_USABLE)

    def test_valid_nested_product_of_abs(self):
        # A composite operand's own operands may themselves be composite.
        body = json.loads(resp())
        body["pointer_arguments"][0] = {
            "id": "arg2", "direction": "in", "extent": "argument",
            "extent_operand": {"size": {"op": "product", "operands": [
                leaf("argument_value", argument_id="arg0"),
                {"op": "abs", "operand": leaf("argument_value", argument_id="arg3")},
            ]}},
        }
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_USABLE)

    def test_schema_invalid_product_wrong_operand_count(self):
        body = json.loads(resp())
        body["pointer_arguments"][0] = {
            "id": "arg2", "direction": "in", "extent": "argument",
            "extent_operand": {"size": {"op": "product", "operands": [
                leaf("argument_value", argument_id="arg0"),
            ]}},
        }
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)

    def test_schema_invalid_abs_with_operands_plural(self):
        body = json.loads(resp())
        body["pointer_arguments"][0]["extent_operand"]["stride"] = {
            "op": "abs", "operands": [{"source": "argument_value", "argument_id": "arg3"}],
        }
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)

    def test_schema_invalid_composite_with_both_op_and_source(self):
        body = json.loads(resp())
        body["pointer_arguments"][0]["extent_operand"]["stride"] = {
            "op": "abs", "source": "argument_value",
            "operand": {"source": "argument_value", "argument_id": "arg3"},
        }
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)

    def test_schema_invalid_unknown_composite_op(self):
        body = json.loads(resp())
        body["pointer_arguments"][0]["extent_operand"]["stride"] = {
            "op": "divide", "operand": {"source": "argument_value", "argument_id": "arg3"},
        }
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)

    def test_schema_invalid_leaf_type_check_applies_inside_composite(self):
        # argument_pointee inside a product must still reference a POINTER
        # argument -- composition must not bypass the per-leaf type check.
        body = json.loads(resp())
        body["pointer_arguments"][0] = {
            "id": "arg2", "direction": "in", "extent": "argument",
            "extent_operand": {"size": {"op": "product", "operands": [
                leaf("argument_value", argument_id="arg0"),
                leaf("argument_pointee", argument_id="arg0"),  # arg0 is scalar, not ptr
            ]}},
        }
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)
        self.assertTrue(any("argument_pointee must reference a pointer" in e for e in r.errors))

    def test_schema_invalid_operand_depth_exceeded(self):
        # Nest well past MAX_OPERAND_DEPTH by wrapping abs(abs(abs(...))).
        op = leaf("argument_value", argument_id="arg0")
        for _ in range(v.MAX_OPERAND_DEPTH + 3):
            op = {"op": "abs", "operand": op}
        body = json.loads(resp())
        body["pointer_arguments"][0] = {
            "id": "arg2", "direction": "in", "extent": "argument",
            "extent_operand": {"size": op},
        }
        r = v.validate_response(json.dumps(body), "f", MANIFEST)
        self.assertEqual(r.state, v.STATE_SCHEMA_INVALID)
        self.assertTrue(any("depth limit" in e for e in r.errors))


if __name__ == "__main__":
    unittest.main()
