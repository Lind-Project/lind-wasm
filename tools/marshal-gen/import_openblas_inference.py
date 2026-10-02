#!/usr/bin/env python3
"""Imports and lowers openblas-inference/final/openblas_inference.json into
gen_grate.py's own internal marshal-record model (issue #22's OpenBLAS
inference-to-runtime integration, Gate 2: "import and lower the unified
inference result").

For a `"source":"static"` record, the unified artifact already stores
exactly gen_grate.py's own schema (marshal-infer's own output) -- passed
through unchanged. For a `"source":"llm","status":"resolved"` record, this
module lowers the LLM's own (differently-shaped) answer into that same
schema, consulting the real function's own lowered-IR signature (the
`llm-prompts/openblas-v6-full/<name>.prompt.json` manifest each function was
actually queried with) to resolve what the unified artifact's own schema
alone cannot: which raw argument slots are plain scalars versus pointers,
and -- for a pointer whose size this importer must determine itself ("one"
and "constant" extents) -- what its pointee actually is.

A `"status":"flagged"` (manually found unsound) or `"status":"unresolved"`
record is never lowered; it stays local with an explicit reason, matching
governing policy #3 and #4 of the OpenBLAS-inference-integration plan.

This module does not itself decide V1/V2 transport eligibility or runtime
support -- that is unmarshalable_reason()'s job (gen_grate.py,
gen_v2_adapter.py), run unchanged against whatever this module lowers. A
record this module successfully lowers can still come back
runtime-unsupported today (e.g. the new "expr" size_kind this module emits
for "argument"-extent pointers has no generator/runtime support until Gate
3 teaches it) -- that gap is exactly what the import report below is for.

Usage:
  import_openblas_inference.py openblas-inference/final/openblas_inference.json \\
      --prompts-dir llm-prompts/openblas-v6-full \\
      --out-marshal openblas-inference/final/openblas_lowered.marshal.json \\
      --out-report openblas-inference/final/import_report.csv
"""
import argparse
import csv
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import gen_grate  # noqa: E402

# ---------------------------------------------------------------------------
# BLAS by-reference scalar naming conventions.
#
# Scoped specifically to the frozen 32-bit (LP64) OpenBLAS build this
# artifact's own inference was run against (llm-prompts/openblas-v6-full's
# manifests, and the real binary compare_static.py/the inference pipeline
# queried) -- NOT a universal claim about Fortran. A Fortran INTEGER is
# only 4 bytes under the LP64 convention; a build configured for ILP64
# (64-bit integers) would make every name in _INT_SCALAR_NAMES below 8
# bytes instead, and this table would need re-deriving against THAT
# build's own manifests before reuse, not assumed to still hold.
#
# "one"-extent pointer_arguments (a single scalar passed by reference, the
# Fortran-BLAS calling convention) mix three completely different pointee
# types under one LLM-schema category: a one-byte CHARACTER flag, a
# 4-byte INTEGER (this build's own fixed width, regardless of the
# function's own single/double precision), or a precision-typed REAL
# scalar (4 or 8 bytes, matching the function's own 's'/'d' prefix). The
# extent category alone cannot distinguish these -- they are all "one
# element" -- so this table, verified against the real argument name used
# by EVERY "one"-extent record in the current artifact (cross-checked
# against each function's own llm-prompts/openblas-v6-full manifest),
# does. An argument name that matches none of these three sets is
# rejected rather than guessed into one of them. _INT_SCALAR_NAMES is also
# reused by lower_operand_tree's own argument_pointee check (see that
# function's doc) for the same reason: it is the only way this importer
# has to distinguish a by-reference INTEGER dimension/stride from a real
# data pointer, since LLVM IR's opaque pointers carry no pointee type.
# ---------------------------------------------------------------------------
_CHAR_FLAG_NAMES = {
    "order", "uplo", "trans", "transa", "transb", "trans_a", "trans_c", "diag", "side",
}
_INT_SCALAR_NAMES = {
    "m", "n", "k", "kl", "ku", "lda", "ldb", "ldc", "incx", "incy", "incz",
    "rows", "cols",
}
_REAL_SCALAR_NAMES = {
    "alpha", "beta", "dd1", "dd2", "dx1", "dx2", "dy1", "dy2", "dparam",
    "sparam", "a", "b", "c", "s", "sb",
}


def precision_bytes(name):
    """The function's own scalar element size (4 for single, 8 for double),
    from its BLAS precision prefix -- verified to be unambiguous for every
    function this artifact's LLM track ever touches (see this module's own
    import-time assertion in main()). `cblas_`-prefixed names carry the
    letter right after that prefix; a Fortran-style name (e.g. `dger_`)
    carries it as its own first letter."""
    rest = name[len("cblas_"):] if name.startswith("cblas_") else name
    m = re.match(r"^([sd])", rest)
    return {"s": 4, "d": 8}[m.group(1)] if m else None


def classify_one_extent_pointee(argname, precision_size):
    """Returns (type_label, byte_size) for a "one"-extent pointer argument,
    or None if `argname` matches none of the three known by-reference
    scalar categories -- callers must reject rather than guess at that
    point (see this module's own header doc)."""
    key = argname.lower()
    if key in _CHAR_FLAG_NAMES:
        return ("char", 1)
    if key in _INT_SCALAR_NAMES:
        return ("int", 4)
    if key in _REAL_SCALAR_NAMES:
        return ("double" if precision_size == 8 else "float", precision_size)
    return None


# ---------------------------------------------------------------------------
# LLM extent-operand tree -> this transport's general expression-tree JSON
# (mirrors lind_marshal.h's struct lind_extent_expr one-to-one; see
# local-notes/active/gate1-extent-expression-contract.md for the runtime
# side of this same contract).
# ---------------------------------------------------------------------------

_LLM_OP_TO_EXPR_OP = {
    "max": "max",
    "product": "product",
    "add": "add",
    "abs": "abs",
    # A plain "divide" (exact division) has no node of its own in the
    # runtime contract: every currently-known caller needing division is a
    # packed-storage footprint of the form n*(n+1)/2, where the dividend is
    # always even, so ceiling and exact division agree -- see
    # lind_marshal.h's LIND_EXPR_CEIL_DIVIDE doc. Decided explicitly, not
    # discovered here: do not widen this silently if a future record's
    # "divide" is NOT provably exact.
    "divide": "ceil_divide",
}

_LLM_SOURCE_TO_EXPR_OP = {
    "argument_value": "arg_value",
    "argument_pointee": "arg_pointee_i32",
    "constant": "constant",
}


class LowerError(ValueError):
    """Raised with a precise, actionable reason whenever an LLM record's
    operand tree cannot be reconciled with this transport's contract --
    caught by the caller and turned into a force_local record with that
    exact reason, never silently repaired or guessed past."""


def _parse_arg_id(argument_id, nargs, what):
    m = re.fullmatch(r"arg(\d+)", argument_id or "")
    if not m:
        raise LowerError(f"{what}: malformed argument id {argument_id!r}")
    idx = int(m.group(1))
    if idx >= nargs:
        raise LowerError(f"{what}: argument index {idx} out of range (nargs={nargs})")
    return idx


def _entry_arg_llvm_type(entry_arguments, idx, what):
    ea = entry_arguments[idx]
    t = ea.get("llvm_type")
    if not t:
        raise LowerError(f"{what}: argument index {idx} has no llvm_type in its manifest")
    return t


def lower_operand_tree(node, entry_arguments, what):
    """Lowers one LLM extent_operand sub-tree (a leaf
    {argument_id,constant_value,source} or an operator {op,operands|operand})
    into {"op": ...} JSON matching lind_extent_expr's own node kinds. Raises
    LowerError with a precise reason for anything that doesn't reconcile --
    including, for a leaf, when the REAL argument it names has the wrong
    ABI type for that leaf kind: an `argument_value` leaf reads a raw scalar
    slot directly, so it must name a genuine scalar (not a pointer, whose
    raw bits are an address, not a count); an `argument_pointee` leaf
    dereferences its argument as a pointer, so it must name a genuine
    pointer (not a scalar, which has nothing to dereference). Guessing past
    either mismatch would silently misinterpret the argument's own raw
    bits -- fail closed instead, the same posture
    _lind_read_arg_leaf_i64 takes on the runtime side of this same
    contract for an out-of-range index."""
    nargs = len(entry_arguments)
    if not isinstance(node, dict):
        raise LowerError(f"{what}: expected an operand object, got {node!r}")

    op = node.get("op")
    if op is not None:
        expr_op = _LLM_OP_TO_EXPR_OP.get(op)
        if expr_op is None:
            raise LowerError(f"{what}: unsupported operator {op!r}")
        if op == "abs":
            operand = node.get("operand")
            if operand is None:
                raise LowerError(f"{what}: abs missing its operand")
            return {"op": "abs", "operand": lower_operand_tree(operand, entry_arguments, what)}
        operands = node.get("operands")
        if not isinstance(operands, list) or len(operands) != 2:
            raise LowerError(f"{what}: {op} needs exactly two operands, got {operands!r}")
        lhs = lower_operand_tree(operands[0], entry_arguments, what)
        rhs = lower_operand_tree(operands[1], entry_arguments, what)
        return {"op": expr_op, "lhs": lhs, "rhs": rhs}

    # Leaf: {argument_id, constant_value, source}.
    source = node.get("source")
    expr_op = _LLM_SOURCE_TO_EXPR_OP.get(source)
    if expr_op is None:
        raise LowerError(f"{what}: unsupported leaf source {source!r}")
    if expr_op == "constant":
        cv = node.get("constant_value")
        if not isinstance(cv, int) or isinstance(cv, bool) or cv < 0:
            raise LowerError(f"{what}: constant leaf has invalid constant_value {cv!r}")
        return {"op": "constant", "value": cv}

    idx = _parse_arg_id(node.get("argument_id"), nargs, what)
    llvm_type = _entry_arg_llvm_type(entry_arguments, idx, what)
    if expr_op == "arg_value":
        # The only scalar width/signedness this dataset's dimension/stride
        # values ever use -- see lind_marshal.h's LIND_EXTENT_LEAF_I32.
        # Emitted explicitly (not left to the runtime's own I32 default) so
        # this check and the runtime contract's own leaf_type field can
        # never silently drift apart.
        if llvm_type != "i32":
            raise LowerError(
                f"{what}: argument_value references argument {idx}, whose real ABI type is "
                f"{llvm_type!r}, not a supported integer scalar (i32) -- refusing to read a "
                f"non-scalar or wrong-width argument as a raw extent value"
            )
        return {"op": "arg_value", "arg_index": idx, "leaf_type": "i32"}

    # expr_op == "arg_pointee_i32"
    if llvm_type != "ptr":
        raise LowerError(
            f"{what}: argument_pointee references argument {idx}, whose real ABI type is "
            f"{llvm_type!r}, not a pointer -- refusing to dereference a non-pointer argument"
        )
    # "ptr" alone only proves the argument IS a pointer, not that it
    # points at an int: LLVM IR's opaque pointers carry no pointee type at
    # all, so this cannot distinguish a genuine `int *n` from a data
    # pointer like `float *x` or `double *x` -- dereferencing the latter
    # as if it were a 4-byte int would read half of a real matrix/vector
    # element as a bogus count. Until a manifest carries an authoritative
    # pointee type, this importer trusts ONLY the fixed, explicitly-scoped
    # set of known OpenBLAS by-reference INTEGER dimension/stride names
    # (the same _INT_SCALAR_NAMES profile "one"-extent classification
    # uses) -- verified to cover every real argument_pointee reference in
    # the current artifact (n, m, k, lda, ldb, ldc, incx, incy, rows,
    # cols). An unrecognized name is rejected, not assumed to be an int.
    pointee_argname = entry_arguments[idx].get("name") or f"arg{idx}"
    if pointee_argname.lower() not in _INT_SCALAR_NAMES:
        raise LowerError(
            f"{what}: argument_pointee references argument {idx} ({pointee_argname!r}), which "
            f"is a pointer but not a name on the known OpenBLAS integer-dimension profile -- "
            f"refusing to assume its pointee is an int rather than real array data"
        )
    return {"op": "arg_pointee_i32", "arg_index": idx}


def _is_precision_scaled_product(node, precision_size):
    """True iff `node` (already-lowered {"op": ...} JSON) is EXACTLY a
    top-level product of the function's own precision-size constant and
    some other (unconstrained) element-count sub-expression -- proving the
    formula actually converts an element count into bytes via
    multiplication by the right factor, not merely containing that number
    somewhere unrelated to the final value (e.g. `product(n, lda) + 4` or
    `ceil_divide(product(n, lda), 4)` each contain the constant 4
    SOMEWHERE but neither one multiplies the element count by it -- both
    must be rejected, not accepted because the number appears in the
    tree). Every correctly-formed "argument"-extent record found in the
    real artifact (cblas_sspr2, dgeadd_, ...) has exactly this shape at the
    top level; this is deliberately a narrow, proven grammar rather than a
    general "is this formula unit-correct" prover."""
    if not isinstance(node, dict) or node.get("op") != "product":
        return False
    def is_precision_const(n):
        return isinstance(n, dict) and n.get("op") == "constant" and n.get("value") == precision_size
    return is_precision_const(node.get("lhs")) or is_precision_const(node.get("rhs"))


def lower_pointer_argument(pa, entry_arguments, precision_size, what):
    """Lowers one LLM `pointer_arguments[]` entry into a gen_grate.py-shaped
    `args[]` PTR entry. Raises LowerError (caller forces the whole function
    local) rather than guess past anything that doesn't reconcile.

    Every returned record carries an explicit `int32_pointee_proven` bool:
    true iff THIS importer has independently proven (via the by-reference
    INTEGER name profile, `_INT_SCALAR_NAMES`) that the pointer's pointee
    is exactly a 4-byte int -- the only fact gen_grate.py's own
    `arg_pointee_i32` validation may trust for a DIFFERENT argument's
    extent formula that dereferences this one. Deliberately a separate
    field from the informational `pointee` descriptor (see the "constant"
    branch below for why `pointee[0]["type"]` alone is not a safety
    proof): a fixed 4-byte buffer is not necessarily an int (a real
    `float *` argument is equally 4 bytes per element), so only this
    explicit, name-profile-verified annotation is trusted downstream."""
    direction = pa.get("direction")
    if direction not in ("in", "out", "inout"):
        raise LowerError(f"{what}: unsupported direction {direction!r}")

    extent = pa.get("extent")
    eo = pa.get("extent_operand")

    if extent == "one":
        # extent_operand is always null here; the pointee's own type/size
        # comes from the real argument's NAME, not from anything the LLM's
        # answer itself carries -- see classify_one_extent_pointee's own
        # doc for why the extent category alone can't tell a char flag, an
        # int dimension, and a precision-typed scalar apart.
        argname = what  # caller passes the real arg name as `what` here
        cls = classify_one_extent_pointee(argname, precision_size)
        if cls is None:
            raise LowerError(
                f"argument {argname!r}: 'one'-extent pointee name not recognized as a "
                f"known by-reference scalar category (char flag / int dimension / "
                f"precision-typed real) -- refusing to guess its size"
            )
        type_label, size = cls
        return {
            "kind": "ptr", "type": "<ptr>", "size": 4, "dir": direction,
            "size_kind": "const", "const_size": size,
            "pointee": [{"kind": "scalar", "type": type_label, "size": size}],
            # Explicit, importer-PROVEN annotation that this pointer's
            # pointee is exactly a 4-byte int -- proven here by the SAME
            # by-reference INTEGER name profile (_INT_SCALAR_NAMES)
            # lower_operand_tree's own argument_pointee branch trusts, not
            # merely implied by `pointee[0]["type"]` (which is informational
            # metadata elsewhere -- see the "constant" branch just below --
            # and must never be silently treated as a safety proof). A
            # consumer that needs to know "is this safe to dereference as
            # a single int" (gen_grate.py's arg_pointee_i32 check) reads
            # THIS field, not the pointee descriptor.
            "int32_pointee_proven": type_label == "int",
        }

    if extent == "constant":
        size_node = (eo or {}).get("size")
        if not isinstance(size_node, dict) or size_node.get("source") != "constant":
            raise LowerError(f"{what}: 'constant' extent must be a plain constant leaf")
        cv = size_node.get("constant_value")
        if not isinstance(cv, int) or isinstance(cv, bool) or cv <= 0:
            raise LowerError(f"{what}: 'constant' extent has invalid constant_value {cv!r}")
        # The byte count is informational-only for the pointee description;
        # the actual copy size is const_size itself (see gen_grate.py's own
        # Emitter: `type` never affects marshalling, only const_size does).
        # A plain 4-byte constant extent is NOT, by itself, proof the
        # pointee is an int -- a real `float *` argument is equally 4
        # bytes per element -- so `int32_pointee_proven` is only set when
        # the real argument NAME also matches the known by-reference
        # INTEGER profile (the same proof "one"-extent classification
        # above already has, applied here too, since a real OpenBLAS int
        # dimension can land in either extent category -- see this
        # module's own README/investigation notes).
        return {
            "kind": "ptr", "type": "<ptr>", "size": 4, "dir": direction,
            "size_kind": "const", "const_size": cv,
            "pointee": [{"kind": "scalar", "type": "opaque", "size": 1}],
            "int32_pointee_proven": cv == 4 and what.lower() in _INT_SCALAR_NAMES,
        }

    if extent == "argument":
        size_node = (eo or {}).get("size")
        if size_node is None:
            raise LowerError(f"{what}: 'argument' extent missing its size operand")
        tree = lower_operand_tree(size_node, entry_arguments, what)
        # "argument" extent is a directly-computed BYTE count (see this
        # module's own header doc and the plan's governing policy: "constant
        # and argument as byte extents"). Accepted only under a narrow,
        # proven grammar -- a top-level product by the function's own
        # precision size (see _is_precision_scaled_product's own doc) --
        # rather than any formula that merely contains the right number
        # somewhere: a formula that only ADDS or DIVIDES by that number
        # (e.g. `product(n, lda) + 4` or `ceil_divide(product(n, lda), 4)`)
        # is still an element count, not a byte count, and must be
        # rejected rather than accepted because the number happens to
        # appear. Reject rather than silently undercount the real transfer
        # -- the exact "missing byte-size multiplier" defect class
        # openblas-inference/README.md's own flagged-function review
        # already found instances of elsewhere (cblas_stbmv's real record
        # is exactly `product(n, lda)`, caught by this same check).
        if precision_size is not None and not _is_precision_scaled_product(tree, precision_size):
            raise LowerError(
                f"{what}: 'argument'-extent byte formula is not a top-level product by the "
                f"function's own precision size ({precision_size}) -- likely an element count "
                f"formula, not a genuine byte formula (see _is_precision_scaled_product's own "
                f"doc for why containing the right number elsewhere in the tree is not enough)"
            )
        return {
            "kind": "ptr", "type": "<ptr>", "size": 4, "dir": direction,
            "size_kind": "expr", "size_expr": tree,
            "pointee": [{"kind": "scalar",
                         "type": "double" if precision_size == 8 else "float",
                         "size": precision_size}],
            # A dynamically-sized byte buffer (the whole point of an
            # "argument"-extent pointer) is never a single scalar int --
            # explicit False, not merely absent, so a consumer never has
            # to guess what a missing field would have meant.
            "int32_pointee_proven": False,
        }

    if extent == "stride_vector":
        if not isinstance(eo, dict) or "size" not in eo or "stride" not in eo:
            raise LowerError(f"{what}: 'stride_vector' extent missing size/stride operand")
        size_tree = lower_operand_tree(eo["size"], entry_arguments, what)
        stride_tree = lower_operand_tree(eo["stride"], entry_arguments, what)
        return {
            "kind": "ptr", "type": "<ptr>", "size": 4, "dir": direction,
            "size_kind": "stride_vector",
            "size_operand_expr": size_tree, "stride_operand_expr": stride_tree,
            "const_size": precision_size,
            "pointee": [{"kind": "scalar",
                         "type": "double" if precision_size == 8 else "float",
                         "size": precision_size}],
            # An array of precision-typed elements, not a single int.
            "int32_pointee_proven": False,
        }

    raise LowerError(f"{what}: unsupported extent kind {extent!r}")


def lower_llm_function(name, rec, manifest):
    """Lowers one LLM-resolved function record into a gen_grate.py-shaped
    function dict, or raises LowerError with a precise reason."""
    entry_args = manifest.get("entry_arguments")
    if not isinstance(entry_args, list) or not entry_args:
        raise LowerError("prompt manifest has no entry_arguments")
    nargs = len(entry_args)

    pointer_args = rec.get("pointer_arguments") or []
    by_index = {}
    for pa in pointer_args:
        idx = _parse_arg_id(pa.get("id"), nargs, name)
        if idx in by_index:
            raise LowerError(f"argument index {idx} is described by more than one pointer_arguments entry")
        by_index[idx] = pa

    precision_size = precision_bytes(name)
    args_out = []
    for i, ea in enumerate(entry_args):
        argname = ea.get("name") or f"arg{i}"
        llvm_type = ea.get("llvm_type")
        if i in by_index:
            # A pointer_arguments entry describes HOW to marshal a
            # pointer; it never establishes BY ITSELF that the argument
            # actually is one. Treating an i32 scalar's raw VALUE as a
            # cross-cage ADDRESS because some record merely claims it has
            # an extent is exactly the kind of misinterpretation the real
            # ABI type must gate, the same posture argument_value/
            # argument_pointee leaves already take inside the formula
            # itself (see lower_operand_tree's own doc).
            if llvm_type != "ptr":
                raise LowerError(
                    f"argument {argname!r} (index {i}) is described by a pointer_arguments "
                    f"entry, but its real ABI type is {llvm_type!r}, not a pointer -- refusing "
                    f"to marshal a scalar's raw value as an address"
                )
            args_out.append(lower_pointer_argument(by_index[i], entry_args, precision_size, argname))
            continue
        if llvm_type == "ptr":
            raise LowerError(
                f"argument {argname!r} (index {i}) is a pointer at the real ABI level "
                f"but is not covered by any pointer_arguments entry"
            )
        scalar_type = {"i32": "int", "float": "float", "double": "double"}.get(llvm_type)
        if scalar_type is None:
            raise LowerError(f"argument {argname!r} (index {i}): unsupported scalar llvm_type {llvm_type!r}")
        size = {"int": 4, "float": 4, "double": 8}[scalar_type]
        args_out.append({"kind": "scalar", "type": scalar_type, "size": size})

    return {
        "name": name,
        "decision": "marshal",
        "ret": {"kind": "void"},  # every LLM-track function in this artifact is void-returning
        "args": args_out,
        "warnings": [
            f"source=llm model={rec.get('model', '?')}: lowered by "
            f"import_openblas_inference.py from openblas-inference/final/openblas_inference.json"
        ],
    }


def import_all(artifact_path, prompts_dir):
    """Returns (functions: dict[name -> gen_grate-shaped record or None],
    report_rows: list[dict]) for every function in the artifact. A function
    lowered to None stays local; its report row carries the reason. Every
    row's generator/runtime-capability columns (`generated_or_local`,
    `selected_transport`) are already filled in via
    _classify_generator_support -- callers never need a separate
    classification pass."""
    with open(artifact_path) as fh:
        artifact = json.load(fh)
    fns = artifact["functions"]

    functions = {}
    report_rows = []

    for name, rec in fns.items():
        source = rec.get("source")
        status = rec.get("status")
        row = {
            "symbol": name,
            "source": source,
            "status": status,
            "selected_transport": "",
            "generated_or_local": "local",
            "reason": "",
        }

        if source == "static" and status == "resolved":
            # Governing policy #1/#2: static proven inference takes
            # precedence, reused unchanged.
            functions[name] = rec["static"]
            report_rows.append(row)
            continue

        if status == "flagged":
            functions[name] = None
            row["reason"] = "flagged as known-unsound by manual review (see openblas-inference/README.md)"
            report_rows.append(row)
            continue

        if status == "unresolved":
            functions[name] = None
            row["reason"] = rec.get("reason") or "unresolved (no LLM answer reached generation)"
            report_rows.append(row)
            continue

        if source == "llm" and status == "resolved":
            manifest_path = os.path.join(prompts_dir, f"{name}.prompt.json")
            try:
                with open(manifest_path) as fh:
                    manifest = json.load(fh)
            except (OSError, json.JSONDecodeError) as e:
                functions[name] = None
                row["reason"] = f"could not read prompt manifest {manifest_path}: {e}"
                report_rows.append(row)
                continue
            try:
                functions[name] = lower_llm_function(name, rec, manifest)
            except LowerError as e:
                functions[name] = None
                row["reason"] = str(e)
            report_rows.append(row)
            continue

        # Anything else is an artifact shape this importer doesn't
        # recognize -- fail closed rather than silently drop it.
        functions[name] = None
        row["reason"] = f"unrecognized source/status combination ({source!r}, {status!r})"
        report_rows.append(row)

    for row in report_rows:
        record = functions[row["symbol"]]
        generated_or_local, cap_result = _classify_generator_support(record)
        row["generated_or_local"] = generated_or_local
        if generated_or_local == "generated":
            row["selected_transport"] = cap_result
        elif not row["reason"]:
            row["reason"] = cap_result or "no generator/runtime capability for this shape yet"

    return functions, report_rows


def _classify_generator_support(record):
    """Returns (generated_or_local, selected_transport_or_v2_reason) for one
    successfully-lowered record, by running it through gen_grate.py's own
    (unmodified) V1/V2 eligibility checks -- the same checks real
    generation uses, not a re-derived approximation. `record is None`
    (already local before reaching this check at all) always reports local
    with no transport."""
    if record is None:
        return "local", ""
    if gen_grate.is_marshalable(record):
        return "generated", "V1"
    v2_reason = gen_grate.unmarshalable_reason(record, max_args=None)
    if v2_reason is None:
        return "generated", "V2"
    return "local", v2_reason


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("artifact", help="openblas-inference/final/openblas_inference.json")
    ap.add_argument("--prompts-dir", required=True, help="llm-prompts/openblas-v6-full")
    ap.add_argument("--out-marshal", required=True, help="output <lib>.marshal.json")
    ap.add_argument("--out-report", required=True, help="output CSV import report")
    args = ap.parse_args()

    functions, report_rows = import_all(args.artifact, args.prompts_dir)

    marshal_out = {
        "functions": [
            {**record, "decision": "marshal"} if record is not None
            else {"name": name, "decision": "force_local",
                  "warnings": [next(r["reason"] for r in report_rows if r["symbol"] == name)]}
            for name, record in sorted(functions.items())
        ]
    }
    with open(args.out_marshal, "w") as fh:
        json.dump(marshal_out, fh, indent=2)
        fh.write("\n")

    with open(args.out_report, "w", newline="") as fh:
        writer = csv.DictWriter(fh, fieldnames=[
            "symbol", "source", "status", "selected_transport", "generated_or_local", "reason",
        ])
        writer.writeheader()
        for row in sorted(report_rows, key=lambda r: r["symbol"]):
            writer.writerow(row)

    total = len(report_rows)
    resolved = sum(1 for r in report_rows if r["status"] == "resolved")
    generated = sum(1 for r in report_rows if r["generated_or_local"] == "generated")
    flagged = sum(1 for r in report_rows if r["status"] == "flagged")
    unresolved = sum(1 for r in report_rows if r["status"] == "unresolved")
    v1 = sum(1 for r in report_rows if r["selected_transport"] == "V1")
    v2 = sum(1 for r in report_rows if r["selected_transport"] == "V2")
    print(f"[import_openblas_inference] {total} functions total "
          f"({resolved} resolved, {flagged} flagged, {unresolved} unresolved)")
    print(f"[import_openblas_inference] {generated}/{resolved} resolved functions are "
          f"actually generator/runtime-supported today ({v1} V1, {v2} V2); "
          f"the rest are local -- see the report's own reason column")
    print(f"[import_openblas_inference] wrote {args.out_marshal}")
    print(f"[import_openblas_inference] wrote {args.out_report}")


if __name__ == "__main__":
    main()
