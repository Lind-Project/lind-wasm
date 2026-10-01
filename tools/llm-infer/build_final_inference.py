#!/usr/bin/env python3
"""Builds the consolidated OpenBLAS inference deliverable:

  <out>/static/openblas.marshal.json   -- copy of the static analysis result
  <out>/llm/llm_results.json           -- one row per LLM-queried function
  <out>/llm/findings_buffer.md         -- copy of the known-issues buffer
  <out>/final/openblas_inference.json  -- the unified, per-function answer

Unification policy, per exported function:
  1. Static "marshal" decision wins outright -- it's a proven symbolic
     result, never second-guessed by an LLM answer.
  2. Otherwise, a `usable` LLM result is used IF the function is not in
     FLAGGED_UNSOUND below.
  3. A function in FLAGGED_UNSOUND keeps status "flagged" -- its LLM
     answer is still included for reference, but status != "resolved",
     so nothing downstream can treat it as trustworthy by accident.
  4. Anything else (not eligible, model_unknown, schema_invalid) is
     "unresolved" with a reason.

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

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
STATIC_JSON = os.path.join(REPO_ROOT, "openblas.marshal.json")
PROMPT_DIR = os.path.join(REPO_ROOT, "llm-prompts", "openblas-v6-full")
SWEEP_CACHE_DIR = os.path.join(REPO_ROOT, "llm-cache", "openblas-sweep")
FINDINGS_BUFFER = os.path.join(REPO_ROOT, "local-notes", "active", "llm-infer-findings-buffer.md")

PROVIDER = "openai"
MODEL = "gpt-6-sol"
PARAMS = {}

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


def build_llm_results(raw_dir):
    """Returns the {function_name: row} summary used for llm_results.json
    and final/openblas_inference.json. As a side effect, writes the FULL
    raw cache entry (meta/raw_response/extracted/validation/usage) for
    every queried function under raw_dir/<function_name>/ -- the
    unprocessed provenance behind each row here, addressed by function
    name instead of the cache's own opaque content hash, so it survives
    independently of llm-cache/ being pruned or regenerated later."""
    cache = cache_mod.Cache(SWEEP_CACHE_DIR)
    results = {}
    for prompt_path in sorted(glob.glob(os.path.join(PROMPT_DIR, "*.prompt.json"))):
        function_name = os.path.basename(prompt_path)[: -len(".prompt.json")]
        with open(prompt_path) as f:
            manifest = json.load(f)
        if not (manifest.get("slice_complete") and manifest.get("eligible_for_llm_inference")):
            results[function_name] = {"eligible": False}
            continue
        key = cache_mod.compute_cache_key(manifest, PROVIDER, MODEL, PARAMS)
        entry = cache.read(key)
        if entry is None:
            results[function_name] = {"eligible": True, "state": "not_queried"}
            continue
        v = entry["validation.json"]
        row = {"eligible": True, "state": v["state"], "provider": PROVIDER, "model": MODEL}
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
    return results


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
        elif llm.get("state") == "usable" and name not in FLAGGED_UNSOUND:
            final[name] = {"source": "llm", "status": "resolved",
                           "pointer_arguments": llm["pointer_arguments"],
                           "model": llm["model"]}
        elif llm.get("state") == "usable" and name in FLAGGED_UNSOUND:
            final[name] = {"source": "llm", "status": "flagged",
                           "reason": FLAGGED_UNSOUND[name],
                           "pointer_arguments": llm["pointer_arguments"],
                           "model": llm["model"]}
        else:
            final[name] = {"source": "none", "status": "unresolved",
                           "reason": f"llm_state={llm.get('state', 'not_queried')}"}
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

    llm_results = build_llm_results(raw_dir)
    with open(os.path.join(llm_dir, "llm_results.json"), "w") as f:
        json.dump({"provider": PROVIDER, "model": MODEL, "params": PARAMS,
                   "functions": llm_results}, f, sort_keys=True, indent=2)
    if os.path.isfile(FINDINGS_BUFFER):
        shutil.copy(FINDINGS_BUFFER, os.path.join(llm_dir, "findings_buffer.md"))

    final = build_final(static_functions, llm_results)
    with open(os.path.join(final_dir, "openblas_inference.json"), "w") as f:
        json.dump({"functions": final}, f, sort_keys=True, indent=2)

    counts = {}
    for row in final.values():
        counts[row["status"]] = counts.get(row["status"], 0) + 1
    print(f"build_final_inference: {len(final)} functions, status counts={counts}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
