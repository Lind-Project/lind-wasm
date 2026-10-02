#!/usr/bin/env python3
"""Builds the consolidated OpenBLAS inference deliverable:

  <out>/static/openblas.marshal.json   -- copy of the static analysis result
  <out>/llm/llm_results.json           -- one row per LLM-queried function
  <out>/llm/findings_buffer.md         -- copy of the known-issues buffer
  <out>/final/openblas_inference.json  -- the unified, per-function answer

Unification policy, per exported function:
  1. Static "marshal" decision wins outright -- it's a proven symbolic
     result, never second-guessed by an LLM answer.
  2. Otherwise, the BEST available LLM result is used IF the function is
     not in FLAGGED_UNSOUND below. "Best" means: any `usable` (schema-
     validated) answer, regardless of which provider/model produced it --
     correctness and validity are what's accepted on, not provenance.
     When more than one model answered the same function, MODEL_PRIORITY
     below picks one deterministically (a tie-break, not a trust
     judgment); which model actually produced the winning answer is
     still recorded for transparency.
  3. A function in FLAGGED_UNSOUND keeps status "flagged" -- its LLM
     answer is still included for reference, but status != "resolved",
     so nothing downstream can treat it as trustworthy by accident.
  4. Anything else (not eligible, or every candidate is model_unknown/
     schema_invalid) is "unresolved" with a reason.

FLAGGED_UNSOUND is a manually-curated list from this session's sweep
review (local-notes/active/llm-infer-findings-buffer.md) -- functions
where a human (not just the schema validator) checked the actual answer
against the real kernel/source and found it unsound, despite passing
validation. This is NOT a substitute for fixing the underlying prompt
issue (the stride_vector-vs-product field confusion, and the
argument-extent-drops-the-byte-multiplier pattern); it's a stopgap so
this deliverable doesn't silently claim a known-wrong answer is final.
"""

import glob
import json
import os
import shutil
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cache as cache_mod
import validator as validator_mod

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
STATIC_JSON = os.path.join(REPO_ROOT, "openblas.marshal.json")
PROMPT_DIR = os.path.join(REPO_ROOT, "llm-prompts", "openblas-v6-full")
SWEEP_CACHE_DIR = os.path.join(REPO_ROOT, "llm-cache", "openblas-sweep")
FINDINGS_BUFFER = os.path.join(REPO_ROOT, "local-notes", "active", "llm-infer-findings-buffer.md")

# A tie-break ONLY, used when more than one provider/model answered the
# same function -- not a trust judgment. Any model not listed sorts last
# (but is still eligible to win if it's the only usable candidate).
MODEL_PRIORITY = ["gpt-6-sol", "gpt-5.6-sol", "gpt-5.5"]

# Confirmed unsound via manual review against real source (the
# stride_vector(dim, raw_leading_dim) field-swap bug, or an
# argument-extent product missing its byte-size multiplier) -- see
# llm-infer-findings-buffer.md for the derivation of each entry.
FLAGGED_UNSOUND = {
    "cblas_dtrsv": "stride_vector field-swap (undercounts by lda-1)",
    "cblas_sger": "stride_vector field-swap (undercounts by lda-1)",
    "cblas_simatcopy": "stride_vector field-swap (undercounts by lda-1)",
    "cblas_stbsv": "stride_vector field-swap (undercounts by lda-1)",
    "dgemmtr_": "argument extent missing byte-size multiplier (undercounts by 8x)",
    "dsymv_": "stride_vector field-swap (undercounts by lda-1)",
    "sgemmt_": "stride_vector field-swap (undercounts by lda-1)",
    "sgemmtr_": "stride_vector field-swap (undercounts by lda-1)",
    "cblas_dgeadd": "stride_vector field-swap (undercounts by lda-1)",
    "cblas_dgemmt": "stride_vector field-swap (undercounts by lda-1)",
    "cblas_domatcopy": "stride_vector field-swap (undercounts by lda-1)",
    "cblas_dgemmtr": "argument extent missing byte-size multiplier (undercounts by 8x)",
}


def _iter_verified_cache_entries(cache_dir):
    """Yields (key, entry) for every entry under cache_dir whose integrity
    verifies against its own integrity.json -- goes through Cache.read()
    (which hashes every file and raises CacheCorrupt on mismatch/missing
    file) rather than opening the JSON files directly, so a corrupted or
    partially-written entry is skipped, never silently trusted."""
    cache = cache_mod.Cache(cache_dir)
    for integrity_path in glob.glob(os.path.join(cache_dir, "*", "*", "integrity.json")):
        key = os.path.basename(os.path.dirname(integrity_path))
        try:
            entry = cache.read(key)
        except cache_mod.CacheCorrupt:
            continue
        if entry is not None:
            yield key, entry


def _state_rank(state):
    # Lower sorts better. Only "usable" is ever accepted as resolved;
    # this ranking just picks which non-usable state to report when no
    # candidate is usable (purely informational -- it is not "resolved"
    # either way).
    order = {validator_mod.STATE_USABLE: 0, validator_mod.STATE_MODEL_UNKNOWN: 1}
    return order.get(state, 2)


def _semantic_rank(entry):
    """(state_rank, model_rank) ONLY -- deliberately excludes the cache
    key. Two candidates with the same semantic rank are genuine ties to
    be grouped and compared, not pre-separated by an identifier that's
    unique by construction (every cache key is a distinct content hash,
    so including it here would make every candidate its own rank and no
    conflict could ever be detected -- see the key's actual, narrower
    job in _group_candidates below)."""
    state = entry["validation.json"]["state"]
    model = entry["meta.json"].get("model")
    model_rank = MODEL_PRIORITY.index(model) if model in MODEL_PRIORITY else len(MODEL_PRIORITY)
    return (_state_rank(state), model_rank)


def _group_candidates(candidates):
    """candidates: [(key, entry), ...] for one function. Returns
    (winning_key, winning_entry, tied, is_conflict):
      - `tied` is every candidate sharing the BEST semantic rank (state,
        model priority) -- not pre-split by key.
      - `is_conflict` is True iff `tied` has more than one DISTINCT
        normalized answer -- genuine disagreement among equally-ranked
        candidates, not merely more than one candidate (duplicates/
        agreeing re-queries are not a conflict).
      - The cache key is used ONLY after grouping, as the final,
        deterministic (not filesystem-order-dependent) tiebreak among
        candidates that already agree -- never to decide what counts as
        "the same rank" in the first place.
    """
    best_rank = min(_semantic_rank(entry) for _, entry in candidates)
    tied = [(key, entry) for key, entry in candidates if _semantic_rank(entry) == best_rank]
    distinct_answers = {json.dumps(entry["validation.json"].get("normalized"), sort_keys=True)
                        for _, entry in tied}
    is_conflict = len(distinct_answers) > 1
    winning_key, winning_entry = min(tied, key=lambda c: c[0])
    return winning_key, winning_entry, tied, is_conflict


def build_llm_results(raw_dir, cache_dir=None, prompt_dir=None):
    """Returns ({function_name: row}, stats) -- the per-function summary
    used for llm_results.json/final/openblas_inference.json, and counts
    of entries rejected along the way. As a side effect, writes the FULL
    raw cache entry (meta/raw_response/extracted/validation/usage) for
    the WINNING candidate of every queried function under
    raw_dir/<function_name>/ -- the unprocessed provenance behind each
    row here, addressed by function name instead of the cache's own
    opaque content hash, so it survives independently of llm-cache/ being
    pruned or regenerated later."""
    cache_dir = cache_dir or SWEEP_CACHE_DIR
    prompt_dir = prompt_dir or PROMPT_DIR

    manifests = {}
    for prompt_path in sorted(glob.glob(os.path.join(prompt_dir, "*.prompt.json"))):
        function_name = os.path.basename(prompt_path)[: -len(".prompt.json")]
        with open(prompt_path) as f:
            manifests[function_name] = json.load(f)

    stats = {"corrupt_entries_skipped": 0, "stale_ir_entries_skipped": 0}
    by_function = {}
    all_keys_seen = 0
    for key, entry in _iter_verified_cache_entries(cache_dir):
        all_keys_seen += 1
        meta = entry["meta.json"]
        function_name = meta.get("function")
        manifest = manifests.get(function_name)
        if manifest is None:
            continue
        # An entry is only a valid candidate for the CURRENT state of this
        # function's evidence -- if marshal-infer's analysis of this
        # function's IR has since changed (a different prompt_hash/
        # input_hash), this answer was produced for a function that no
        # longer exists in this exact form, and must never be silently
        # reused as if it still applied.
        if meta.get("prompt_hash") != manifest.get("prompt_hash") or \
           meta.get("input_hash") != manifest.get("input_hash"):
            stats["stale_ir_entries_skipped"] += 1
            continue
        by_function.setdefault(function_name, []).append((key, entry))
    stats["corrupt_entries_skipped"] = (
        len(glob.glob(os.path.join(cache_dir, "*", "*", "integrity.json"))) - all_keys_seen
    )

    results = {}
    for function_name, manifest in sorted(manifests.items()):
        if not (manifest.get("slice_complete") and manifest.get("eligible_for_llm_inference")):
            results[function_name] = {"eligible": False}
            continue
        candidates = by_function.get(function_name)
        if not candidates:
            results[function_name] = {"eligible": True, "state": "not_queried"}
            continue

        winning_key, entry, tied, is_conflict = _group_candidates(candidates)

        v = entry["validation.json"]
        winning_meta = entry["meta.json"]
        row = {"eligible": True, "state": v["state"],
              "provider": winning_meta.get("provider"), "model": winning_meta.get("model"),
              "candidates_considered": len(candidates)}
        if is_conflict:
            # Equally-ranked candidates whose actual answers disagree --
            # a deterministic tiebreak still picks `entry` above, but the
            # disagreement itself must be visible, never silently
            # resolved as if it were an ordinary single-candidate result.
            row["conflicting_candidates"] = [key for key, _ in tied]
        if v.get("normalized"):
            row["pointer_arguments"] = v["normalized"]["pointer_arguments"]
        if function_name in FLAGGED_UNSOUND:
            row["flagged_unsound_reason"] = FLAGGED_UNSOUND[function_name]
        results[function_name] = row

        fn_dir = os.path.join(raw_dir, function_name)
        os.makedirs(fn_dir, exist_ok=True)
        for name in ("meta.json", "raw_response.json", "validation.json", "usage.json"):
            with open(os.path.join(fn_dir, name), "w") as f:
                json.dump(entry[name], f, sort_keys=True, indent=2)
        with open(os.path.join(fn_dir, "extracted.txt"), "w") as f:
            f.write(entry["extracted.txt"])
    return results, stats


def build_final(static_functions, llm_results):
    final = {}
    for name, static_fn in static_functions.items():
        if static_fn.get("decision") == "marshal":
            final[name] = {"source": "static", "status": "resolved", "static": static_fn}
            continue
        llm = llm_results.get(name, {"eligible": False})
        if not llm.get("eligible"):
            final[name] = {"source": "none", "status": "unresolved",
                           "reason": "not eligible for LLM inference (static evidence incomplete)"}
        elif llm.get("state") != "usable":
            final[name] = {"source": "none", "status": "unresolved",
                           "reason": f"llm_state={llm.get('state', 'not_queried')}"}
        elif llm.get("conflicting_candidates"):
            # Equally-ranked usable candidates disagree -- never "resolved"
            # no matter which one the deterministic tiebreak happened to
            # pick; needs manual review, same as a known-unsound answer.
            final[name] = {"source": "llm", "status": "flagged",
                           "reason": "multiple equally-ranked usable candidates disagree",
                           "conflicting_candidates": llm["conflicting_candidates"],
                           "pointer_arguments": llm["pointer_arguments"],
                           "model": llm["model"]}
        elif name in FLAGGED_UNSOUND:
            final[name] = {"source": "llm", "status": "flagged",
                           "reason": FLAGGED_UNSOUND[name],
                           "pointer_arguments": llm["pointer_arguments"],
                           "model": llm["model"]}
        else:
            final[name] = {"source": "llm", "status": "resolved",
                           "pointer_arguments": llm["pointer_arguments"],
                           "model": llm["model"]}
    return final


def main():
    out_dir = sys.argv[1] if len(sys.argv) > 1 else os.path.join(REPO_ROOT, "openblas-inference")
    static_dir = os.path.join(out_dir, "static")
    llm_dir = os.path.join(out_dir, "llm")
    raw_dir = os.path.join(llm_dir, "raw")
    final_dir = os.path.join(out_dir, "final")
    for d in (static_dir, llm_dir, raw_dir, final_dir):
        os.makedirs(d, exist_ok=True)

    with open(STATIC_JSON) as f:
        static_data = json.load(f)
    static_functions = {fn["name"]: fn for fn in static_data["functions"]}
    shutil.copy(STATIC_JSON, os.path.join(static_dir, "openblas.marshal.json"))

    llm_results, llm_stats = build_llm_results(raw_dir)
    with open(os.path.join(llm_dir, "llm_results.json"), "w") as f:
        json.dump({"acceptance_policy": "any validated usable answer, regardless of provider/model "
                                       "(see module docstring); provider/model recorded per-function "
                                       "for transparency, not as an acceptance criterion",
                   "model_priority_tiebreak": MODEL_PRIORITY,
                   "build_stats": llm_stats,
                   "functions": llm_results}, f, sort_keys=True, indent=2)
    if os.path.isfile(FINDINGS_BUFFER):
        shutil.copy(FINDINGS_BUFFER, os.path.join(llm_dir, "findings_buffer.md"))

    final = build_final(static_functions, llm_results)
    with open(os.path.join(final_dir, "openblas_inference.json"), "w") as f:
        json.dump({"functions": final}, f, sort_keys=True, indent=2)

    counts = {}
    conflicts = [name for name, row in llm_results.items() if row.get("conflicting_candidates")]
    for row in final.values():
        counts[row["status"]] = counts.get(row["status"], 0) + 1
    print(f"build_final_inference: {len(final)} functions, status counts={counts}, "
          f"llm_build_stats={llm_stats}, functions_with_conflicting_candidates={conflicts}",
          file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
