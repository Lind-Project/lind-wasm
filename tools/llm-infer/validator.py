"""Validation for marshal-response-v6 answers.

This is the single authoritative interpreter of the response contract on the
API-runner side (see local-notes/active/plan-llm-api-inference.md section 6):
llm_query.py and compare_static.py both call into this module rather than
re-deriving the enum/shape rules themselves. The contract itself -- what a
valid answer looks like -- is defined by marshal-infer's own prompt
generation (tools/marshal-infer/src/LlmPrompt.cpp); this module enforces it
on the received text, it does not redefine it.

A response is parsed as EXACTLY one JSON object: no Markdown-fence
stripping, no substring extraction, no repair of malformed JSON. A model
that wraps its answer in prose or a code fence has produced an unusable
response, not one this tool should salvage.
"""

import json

RESPONSE_SCHEMA_VERSION = "marshal-response-v6"

DIRECTIONS = frozenset({"in", "out", "inout", "unknown"})
EXTENTS = frozenset({"one", "constant", "argument", "c_string", "stride_vector", "unknown"})
OPERAND_SOURCES = frozenset({"argument_value", "argument_pointee", "constant"})
# "divide" always means CEILING division -- it is used for an exact
# closed-form size formula (e.g. packed-triangular storage's N*(N+1)/2),
# never a lossy approximation, so rounding UP is the only choice that can
# never undercount a real access.
COMPOSITE_OPS = frozenset({"product", "abs", "max", "add", "divide"})

# Extents that carry no extent_operand at all.
EXTENTS_WITHOUT_OPERAND = frozenset({"one", "c_string", "unknown"})
# Extents whose extent_operand is {"size": <operand>} -- a single NAMED
# operand slot, the same shape family as stride_vector's {"size": ...,
# "stride": ...} rather than a bare operand. Every extent_operand is a
# named-key object, never sometimes-bare and sometimes-wrapped, so a
# consumer never has to know which extent kind it's looking at before it
# can even parse the operand shape.
EXTENTS_WITH_SINGLE_OPERAND = frozenset({"constant", "argument"})
# stride_vector's extent_operand is {"size": <operand>, "stride": <operand>}.

MAX_REPRESENTABLE_CONSTANT = 2**64 - 1  # matches ParamTree.h's ExtentOperand::constValue (uint64_t)
# Real cases need at most a few levels of composition (e.g. the
# packed-triangular formula divide(product(N, add(N, 1)), 2) is 3 levels
# deep) -- bounded generously above that to reject a pathological response
# without an arbitrary-depth-recursion concern, not to accommodate any
# known real shape deeper than this.
MAX_OPERAND_DEPTH = 4

# Terminal states -- see plan section 6. Exactly these six values, nothing else.
STATE_USABLE = "usable"
STATE_MODEL_UNKNOWN = "model_unknown"
STATE_SCHEMA_INVALID = "schema_invalid"
STATE_JSON_INVALID = "json_invalid"
STATE_API_FAILED = "api_failed"
STATE_CACHE_CORRUPT = "cache_corrupt"


class ValidationResult:
    """One validated response. `normalized` is set (a dict with pointer_arguments
    sorted by id) whenever `state` is USABLE or MODEL_UNKNOWN -- i.e. whenever
    the response parsed as exactly one JSON object AND matched the schema,
    regardless of whether its content is fully decided. `errors` is set only
    for SCHEMA_INVALID/JSON_INVALID, as a list of human-readable reasons."""

    def __init__(self, state, normalized=None, errors=None):
        self.state = state
        self.normalized = normalized
        self.errors = errors or []

    def to_dict(self):
        d = {"state": self.state}
        if self.normalized is not None:
            d["normalized"] = self.normalized
        if self.errors:
            d["errors"] = self.errors
        return d


def _is_plain_int(v):
    # bool is a subclass of int in Python; a JSON `true`/`false` must never
    # be accepted where an integer constant is expected.
    return isinstance(v, int) and not isinstance(v, bool)


def _pointer_arg_ids(manifest):
    return {a["id"] for a in manifest["entry_arguments"] if a["llvm_type"] == "ptr"}


def _arg_llvm_type(manifest, arg_id):
    for a in manifest["entry_arguments"]:
        if a["id"] == arg_id:
            return a["llvm_type"]
    return None


def _validate_operand(op, manifest, errors, where, depth=0):
    """An operand slot is filled by EITHER a leaf ({"source": ...}) or a
    composite ({"op": "product"|"max"|"add"|"divide"|"abs", ...}) wrapping
    further operand(s) of the same two-way choice -- see LlmPrompt.cpp's
    own prompt text for the exact contract this mirrors. "source" and "op"
    are mutually exclusive discriminators; a dict must match exactly one
    shape. "divide"'s operands are [dividend, divisor], CEILING division --
    see COMPOSITE_OPS' own comment on why that rounding direction is fixed
    rather than left to the response to pick."""
    if not isinstance(op, dict):
        errors.append(f"{where}: extent operand is not an object")
        return
    if depth > MAX_OPERAND_DEPTH:
        errors.append(f"{where}: operand nesting exceeds depth limit")
        return

    if "op" in op:
        kind = op["op"]
        if kind not in COMPOSITE_OPS:
            errors.append(f"{where}: composite op {kind!r} is not one of {sorted(COMPOSITE_OPS)}")
            return
        if "source" in op:
            errors.append(f"{where}: an operand cannot carry both 'op' and 'source'")
            return
        if kind in ("product", "max", "add", "divide"):
            operands = op.get("operands")
            if "operand" in op or not isinstance(operands, list) or len(operands) != 2:
                errors.append(f"{where}: {kind!r} must carry exactly {{'operands': [<operand>, <operand>]}}")
                return
            if kind == "divide":
                divisor = operands[1]
                if isinstance(divisor, dict) and divisor.get("source") == "constant" and divisor.get("constant_value") == 0:
                    errors.append(f"{where}: 'divide' cannot carry a literal zero divisor")
                    return
            for i, sub in enumerate(operands):
                _validate_operand(sub, manifest, errors, f"{where}.operands[{i}]", depth + 1)
        else:  # "abs"
            if "operands" in op or "operand" not in op:
                errors.append(f"{where}: 'abs' must carry exactly {{'operand': <operand>}}")
                return
            _validate_operand(op["operand"], manifest, errors, f"{where}.operand", depth + 1)
        return

    # Leaf: {"source": "argument_value"|"argument_pointee"|"constant", ...}
    source = op.get("source")
    if source not in OPERAND_SOURCES:
        errors.append(f"{where}: operand source {source!r} is not one of {sorted(OPERAND_SOURCES)}")
        return
    # provider.py's RESPONSE_SCHEMA_JSON_SCHEMA requires both keys on every
    # leaf, with the unused one explicit null rather than omitted -- the
    # same contract is enforced here so a hand-built (non-strict-mode)
    # response cannot slip an omitted key past this validator that the real
    # schema would have rejected.
    if "argument_id" not in op or "constant_value" not in op:
        errors.append(f"{where}: operand leaf must carry both argument_id and constant_value "
                      f"(null for the one 'source' does not use)")
        return
    has_arg = op["argument_id"] is not None
    has_const = op["constant_value"] is not None
    if source == "constant":
        if has_arg or not has_const:
            errors.append(f"{where}: a 'constant' operand must carry constant_value only")
            return
        v = op["constant_value"]
        if not _is_plain_int(v) or v < 0 or v > MAX_REPRESENTABLE_CONSTANT:
            errors.append(f"{where}: constant_value {v!r} is not a representable non-negative integer")
        return
    # argument_value / argument_pointee
    if has_const or not has_arg:
        errors.append(f"{where}: a {source!r} operand must carry argument_id only")
        return
    arg_id = op["argument_id"]
    llvm_type = _arg_llvm_type(manifest, arg_id)
    if llvm_type is None:
        errors.append(f"{where}: argument_id {arg_id!r} does not exist in the manifest")
        return
    is_pointer = llvm_type == "ptr"
    if source == "argument_pointee" and not is_pointer:
        errors.append(f"{where}: argument_pointee must reference a pointer argument, {arg_id!r} is {llvm_type!r}")
    if source == "argument_value" and is_pointer:
        errors.append(f"{where}: argument_value must reference a scalar argument, {arg_id!r} is a pointer")


def validate_response(raw_text, function_name, manifest):
    """Validates `raw_text` (the extracted response text from a provider
    call) against `manifest` (the parsed *.prompt.json for `function_name`).
    Never raises for a malformed response -- every failure mode is reported
    as a ValidationResult with a terminal state, per plan section 6."""
    try:
        obj = json.loads(raw_text)
    except (json.JSONDecodeError, TypeError, ValueError):
        return ValidationResult(STATE_JSON_INVALID, errors=["response is not valid JSON"])
    if not isinstance(obj, dict):
        return ValidationResult(STATE_JSON_INVALID, errors=["response JSON is not a single object"])

    errors = []

    if obj.get("response_schema_version") != RESPONSE_SCHEMA_VERSION:
        errors.append(
            f"response_schema_version {obj.get('response_schema_version')!r} != {RESPONSE_SCHEMA_VERSION!r}"
        )
    if obj.get("function") != function_name:
        errors.append(f"function {obj.get('function')!r} != requested {function_name!r}")

    entries = obj.get("pointer_arguments")
    if not isinstance(entries, list):
        errors.append("pointer_arguments is missing or not a list")
        entries = []

    expected_ids = _pointer_arg_ids(manifest)
    seen_ids = []
    any_unknown = False
    normalized_entries = []

    for i, entry in enumerate(entries):
        where = f"pointer_arguments[{i}]"
        if not isinstance(entry, dict):
            errors.append(f"{where}: not an object")
            continue
        arg_id = entry.get("id")
        if arg_id is None:
            errors.append(f"{where}: missing id")
            continue
        if arg_id not in expected_ids:
            errors.append(f"{where}: id {arg_id!r} is not a boundary pointer argument of {function_name!r}")
        seen_ids.append(arg_id)

        direction = entry.get("direction")
        if direction not in DIRECTIONS:
            errors.append(f"{where} ({arg_id}): direction {direction!r} not in {sorted(DIRECTIONS)}")
            direction = None
        extent = entry.get("extent")
        if extent not in EXTENTS:
            errors.append(f"{where} ({arg_id}): extent {extent!r} not in {sorted(EXTENTS)}")
            extent = None

        operand = entry.get("extent_operand")
        if extent in EXTENTS_WITHOUT_OPERAND:
            if operand is not None:
                errors.append(f"{where} ({arg_id}): extent {extent!r} must not carry extent_operand")
        elif extent in EXTENTS_WITH_SINGLE_OPERAND:
            if not isinstance(operand, dict) or "size" not in operand:
                errors.append(f"{where} ({arg_id}): extent {extent!r} requires extent_operand.size")
            else:
                _validate_operand(operand["size"], manifest, errors, f"{where} ({arg_id}) extent_operand.size")
        elif extent == "stride_vector":
            if not isinstance(operand, dict) or "size" not in operand or "stride" not in operand:
                errors.append(f"{where} ({arg_id}): stride_vector requires extent_operand.size and .stride")
            else:
                _validate_operand(operand["size"], manifest, errors, f"{where} ({arg_id}) extent_operand.size")
                _validate_operand(operand["stride"], manifest, errors, f"{where} ({arg_id}) extent_operand.stride")

        if direction == "unknown" or extent == "unknown":
            any_unknown = True

        normalized_entries.append(entry)

    if len(seen_ids) != len(set(seen_ids)):
        errors.append("pointer_arguments contains a duplicate id")
    missing = expected_ids - set(seen_ids)
    if missing:
        errors.append(f"pointer_arguments is missing required id(s): {sorted(missing)}")
    extra = set(seen_ids) - expected_ids
    if extra:
        errors.append(f"pointer_arguments references non-pointer or nonexistent id(s): {sorted(extra)}")

    if errors:
        return ValidationResult(STATE_SCHEMA_INVALID, errors=errors)

    normalized = {
        "response_schema_version": obj["response_schema_version"],
        "function": obj["function"],
        "pointer_arguments": sorted(normalized_entries, key=lambda e: e["id"]),
    }
    if any_unknown:
        return ValidationResult(STATE_MODEL_UNKNOWN, normalized=normalized)
    return ValidationResult(STATE_USABLE, normalized=normalized)
