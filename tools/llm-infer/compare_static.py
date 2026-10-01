#!/usr/bin/env python3
"""Compares static marshal-infer decisions against validated LLM answers.
See local-notes/active/plan-llm-api-inference.md section 8.

This is a SEPARATE local analysis tool: it reads the marshal JSON already
produced by marshal-infer's own supported CLI (`marshal-infer --json ...`,
or infer_openblas.sh which wraps it) and the LLM cache llm_query.py already
produced -- it never invokes marshal-infer itself, imports Infer.cpp, or
duplicates its analysis. Producing the static JSON is the caller's job,
exactly as marshal-infer's own supported entry points already do it.

Static agreement increases confidence but is NOT independent ground truth.
A function unsupported by static inference (decision=force_local) is an
UNVERIFIED LLM CANDIDATE if the LLM produced a usable answer for it -- this
tool never labels such a case "correct", only "static-unresolved".

Reports one extra category beyond section 8's six: `neither_available`, for
a pointer argument where static is force_local AND the LLM produced no
usable classification either (never queried, or queried and unusable) --
section 8's six categories all implicitly assume at least one side produced
something to report on.
"""

import argparse
import glob
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cache as cache_mod
import validator as validator_mod

CATEGORY_EXACT = "exact_agreement"
CATEGORY_EQUIVALENT = "representationally_equivalent"
CATEGORY_DIRECTION_DISAGREE = "direction_disagreement"
CATEGORY_EXTENT_DISAGREE = "extent_disagreement"
CATEGORY_LLM_ONLY = "llm_usable_static_unresolved"
CATEGORY_STATIC_ONLY = "static_available_llm_unusable"
CATEGORY_NEITHER_AVAILABLE = "neither_available"  # not one of section 8's six -- see module docstring


def _leaf(source, argument_id=None, constant_value=None):
    """A leaf operand with BOTH keys always present (None for the one
    `source` does not use) -- the same shape validator.py normalizes every
    LLM leaf into, so a static-derived operand compares equal (==) to a
    normalized LLM one whenever they agree, rather than differing merely
    because one side omitted a key the other carries as null."""
    return {"source": source, "argument_id": argument_id, "constant_value": constant_value}


def normalize_static_ptr_arg(static_arg):
    """Maps one static per-argument record (kind=='ptr', see ParamTree.h's
    SizeKind + main.cpp's jsonNode) onto (direction, extent, extent_operand,
    unmappable_reason). extent_operand is always a named-key object
    ({"size": <operand>}, or {"size": ..., "stride": ...} for stride_vector),
    matching marshal-response-v6's shape so it compares directly against a
    normalized LLM entry. `unmappable_reason` is set instead of extent/operand
    when the static classification has no marshal-response-v6 equivalent at
    all (e.g. PtrArray -- a NULL-terminated pointer array, like argv/envp,
    which the LLM schema has no vocabulary for)."""
    direction = static_arg.get("dir")
    kind = static_arg.get("size_kind")
    if kind == "const":
        size = _leaf("constant", constant_value=static_arg["const_size"])
        return direction, "constant", {"size": size}, None
    if kind == "cstr":
        return direction, "c_string", None, None
    if kind == "from_arg":
        arg_id = f"arg{static_arg['size_arg_index']}"
        size = _leaf("argument_value", argument_id=arg_id)
        return direction, "argument", {"size": size}, None
    if kind == "from_arg_pointee":
        arg_id = f"arg{static_arg['size_arg_index']}"
        size = _leaf("argument_pointee", argument_id=arg_id)
        return direction, "argument", {"size": size}, None
    if kind == "stride_vector":
        def op(o):
            src = o["source"]
            if src == "constant":
                return _leaf("constant", constant_value=o["const_value"])
            mapped = "argument_value" if src == "value" else "argument_pointee"
            return _leaf(mapped, argument_id=f"arg{o['arg_index']}")
        operand = {"size": op(static_arg["size_operand"]), "stride": op(static_arg["stride_operand"])}
        return direction, "stride_vector", operand, None
    return direction, None, None, f"static size_kind {kind!r} has no marshal-response-v6 equivalent"


def compare_argument(static_arg, llm_entry, llm_was_queried):
    """static_arg: the static JSON's args[i] record, or None if this
    function's static decision was force_local (no per-argument data at
    all). llm_entry: the normalized pointer_arguments[] entry for this arg
    id, or None if the LLM's cached attempt didn't classify it (unusable
    state, or no cache entry). llm_was_queried: a diagnostic detail only
    (never queried at all, vs. queried and unusable) -- it must NEVER be
    used to pick between CATEGORY_STATIC_ONLY and CATEGORY_LLM_ONLY, since
    those two are already fully determined by whether static_arg/llm_entry
    themselves are present; conflating the two caused a real bug during
    development (a force_local function with a failed LLM query was
    mislabeled "static available" when static had nothing at all)."""
    if static_arg is None and llm_entry is None:
        return CATEGORY_NEITHER_AVAILABLE, {"llm_was_queried": llm_was_queried}
    if static_arg is None:
        return CATEGORY_LLM_ONLY, {"llm_extent": llm_entry.get("extent")}
    s_dir, s_extent, s_operand, unmappable = normalize_static_ptr_arg(static_arg)
    if llm_entry is None:
        return CATEGORY_STATIC_ONLY, {"static_extent": s_extent or unmappable, "llm_was_queried": llm_was_queried}
    if unmappable:
        return CATEGORY_EXTENT_DISAGREE, {"reason": unmappable, "llm_extent": llm_entry.get("extent")}

    l_dir = llm_entry.get("direction")
    l_extent = llm_entry.get("extent")
    l_operand = llm_entry.get("extent_operand")

    if l_dir != s_dir:
        return CATEGORY_DIRECTION_DISAGREE, {"static_direction": s_dir, "llm_direction": l_dir}
    if l_extent == s_extent and l_operand == s_operand:
        return CATEGORY_EXACT, {}
    # No representationally-equivalent rule is implemented yet: converting
    # between e.g. a stride_vector's element-count operand and a flat byte
    # count would need the pointee's element size, which neither this
    # report's inputs carry. Adding a rule here without a concrete observed
    # case to justify it would risk manufacturing false agreement -- see
    # this project's own PATTERNS.md discipline of only encoding a pattern
    # once a real example motivates it. Until then, anything not identical
    # after normalization is reported as a real disagreement.
    if l_extent != s_extent:
        return CATEGORY_EXTENT_DISAGREE, {"static_extent": s_extent, "llm_extent": l_extent}
    return CATEGORY_EXTENT_DISAGREE, {"static_operand": s_operand, "llm_operand": l_operand}


def load_static_functions(static_json_path):
    with open(static_json_path) as f:
        data = json.load(f)
    return {fn["name"]: fn for fn in data["functions"]}


def find_cached_result(cache, manifest, provider, model, params):
    """Returns (was_queried, normalized_pointer_args_by_id) for `manifest`'s
    function. normalized_pointer_args_by_id is {} if never queried or the
    cached attempt was not in a state that carries a normalized response."""
    key = cache_mod.compute_cache_key(manifest, provider, model, params)
    entry = cache.read(key)
    if entry is None:
        return False, {}
    validation = entry["validation.json"]
    normalized = validation.get("normalized")
    if not normalized:
        return True, {}
    return True, {e["id"]: e for e in normalized["pointer_arguments"]}


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--static-json", required=True)
    ap.add_argument("--prompt-dir", required=True)
    ap.add_argument("--cache-dir", required=True)
    ap.add_argument("--provider", required=True)
    ap.add_argument("--model", required=True)
    ap.add_argument("--param", action="append")
    ap.add_argument("--report", required=True)
    args = ap.parse_args()

    params = {}
    for p in args.param or []:
        k, _, v = p.partition("=")
        try:
            params[k] = json.loads(v)
        except json.JSONDecodeError:
            params[k] = v

    static_functions = load_static_functions(args.static_json)
    cache = cache_mod.Cache(args.cache_dir)

    per_category_count = {}
    per_function = []

    for json_path in sorted(glob.glob(os.path.join(args.prompt_dir, "*.prompt.json"))):
        function_name = os.path.basename(json_path)[: -len(".prompt.json")]
        with open(json_path) as f:
            manifest = json.load(f)

        static_fn = static_functions.get(function_name)
        static_marshals = bool(static_fn and static_fn.get("decision") == "marshal")
        was_queried, llm_by_id = find_cached_result(cache, manifest, args.provider, args.model, params)

        pointer_ids = [a["id"] for a in manifest["entry_arguments"] if a["llvm_type"] == "ptr"]
        arg_results = []
        for arg_id in pointer_ids:
            arg_index = int(arg_id[len("arg"):])
            static_arg = static_fn["args"][arg_index] if static_marshals else None
            llm_entry = llm_by_id.get(arg_id)
            category, detail = compare_argument(static_arg, llm_entry, was_queried)
            per_category_count[category] = per_category_count.get(category, 0) + 1
            arg_results.append({"id": arg_id, "category": category, "detail": detail})

        per_function.append({
            "function": function_name,
            "static_decision": static_fn["decision"] if static_fn else "not_in_static_json",
            "llm_queried": was_queried,
            "arguments": arg_results,
        })

    report = {
        "static_json": os.path.abspath(args.static_json),
        "prompt_dir": os.path.abspath(args.prompt_dir),
        "provider": args.provider,
        "model": args.model,
        "params": params,
        "counts_by_category": per_category_count,
        "functions": per_function,
    }
    with open(args.report, "w") as f:
        json.dump(report, f, sort_keys=True, indent=2)
    print(f"compare_static: {len(per_function)} functions, categories={per_category_count}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
