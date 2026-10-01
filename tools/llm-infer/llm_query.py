#!/usr/bin/env python3
"""Runs an LLM query for every eligible prompt in a marshal-infer
--llm-prompt-only output directory, through a content-addressed cache. See
local-notes/active/plan-llm-api-inference.md for the full design this
follows. This stage stops before converting a validated response into
marshal metadata or trusting it at runtime -- it only queries, validates,
caches, and reports.

The API credential is read ONLY from the LLM_INFER_API_KEY environment
variable -- never accept it as a command-line argument.
"""

import argparse
import datetime
import glob
import hashlib
import json
import os
import sys
from concurrent.futures import ThreadPoolExecutor, as_completed

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import cache as cache_mod
import provider as provider_mod
import validator as validator_mod

SUPPORTED_PROMPT_VERSIONS = {"marshal-ir-v2"}
SUPPORTED_RESPONSE_SCHEMA_VERSIONS = {"marshal-response-v6"}

# States a cache entry can already be in that a NORMAL rerun (no --refresh,
# no --retry-invalid) treats as a complete, billed attempt -- reusing it
# spends zero additional tokens. api_failed is deliberately excluded: it
# represents an attempt that never produced a billed model answer, so a
# resumed run always retries it (see plan section 5's resumability
# guarantee), independent of either flag.
_COMPLETE_STATES = {
    validator_mod.STATE_USABLE,
    validator_mod.STATE_MODEL_UNKNOWN,
    validator_mod.STATE_SCHEMA_INVALID,
    validator_mod.STATE_JSON_INVALID,
}


def dispatch_with_warmup(queryable, process_one_fn, warmup_enabled):
    """Runs a leading portion of `queryable` SEQUENTIALLY through
    `process_one_fn(function_name, manifest)`, stopping as soon as one
    attempt actually reaches the provider (cache_hit is False), and returns
    (results_so_far, remaining_queryable) for the caller to dispatch the
    rest however it likes (e.g. a thread pool).

    A local cache hit makes no request at all, so it warms nothing -- if the
    batch's first few functions all happen to be re-runs of an
    already-cached answer, warm-up must keep walking forward past them
    rather than declaring victory after one no-op call. If every function in
    `queryable` is a cache hit, the whole list is consumed this way and
    `remaining` is empty; that's correct, since there is no provider-side
    cache to warm when no request is ever made."""
    if not warmup_enabled or len(queryable) <= 1:
        return [], queryable
    results = []
    for i, (fn, manifest) in enumerate(queryable):
        r = process_one_fn(fn, manifest)
        results.append(r)
        if not r["cache_hit"]:
            return results, queryable[i + 1:]
    return results, []


def _now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def _sha256_hex(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def discover(prompt_dir):
    """Yields (function_name, prompt_txt_path, prompt_json_path) for every
    *.prompt.json in prompt_dir. A manifest with no matching *.prompt.txt is
    still yielded (function_name derived purely from the .json's own name)
    so local_eligibility_check can report the missing pair explicitly,
    rather than silently skipping it."""
    for json_path in sorted(glob.glob(os.path.join(prompt_dir, "*.prompt.json"))):
        function_name = os.path.basename(json_path)[: -len(".prompt.json")]
        txt_path = json_path[: -len(".json")] + ".txt"
        yield function_name, txt_path, json_path


def local_eligibility_check(txt_path, json_path):
    """Returns (manifest, None) if this pair is queryable, or (None, reason)
    if it must be rejected locally without ever making a request -- see
    plan section 2's list. `reason` is one of a small fixed set of strings
    used verbatim in the run summary."""
    if not os.path.isfile(txt_path):
        return None, "missing_prompt_txt"
    with open(json_path) as f:
        manifest = json.load(f)
    with open(txt_path) as f:
        prompt_text = f.read()

    if manifest.get("prompt_version") not in SUPPORTED_PROMPT_VERSIONS:
        return None, "unsupported_prompt_version"
    if manifest.get("response_schema_version") not in SUPPORTED_RESPONSE_SCHEMA_VERSIONS:
        return None, "unsupported_response_schema_version"
    if not manifest.get("slice_complete") or not manifest.get("eligible_for_llm_inference"):
        return None, "ineligible_manifest"

    expected_prompt_hash = manifest.get("prompt_hash", "")
    if not expected_prompt_hash.startswith("sha256:"):
        return None, "hash_mismatch"
    if "sha256:" + _sha256_hex(prompt_text) != expected_prompt_hash:
        return None, "hash_mismatch"

    manifest["_prompt_text"] = prompt_text
    return manifest, None


def process_one(function_name, manifest, cache, provider, provider_name, model, params,
                max_attempts, timeout, refresh, retry_invalid, enable_prompt_caching=False):
    """Runs the full lock -> cache-check -> (query) -> validate -> cache-write
    sequence for one function. Returns a small result dict for the summary;
    never raises for an ordinary API/validation failure (those are terminal
    states, not exceptions) -- only a cache corruption propagates, since
    that is not a state this function can safely paper over.

    `enable_prompt_caching` (default off) gates whether the request is split
    into a system/user message pair with a `prompt_cache_key`, for
    OpenAI's provider-side prompt caching. Default OFF because a direct
    A/B comparison found this splitting can change the model's answer on
    at least one real function (cblas_dcopy) -- not reliably for the
    worse, but the effect is real, so the lower-risk request shape (one
    flat user message, matching how a human would paste the same prompt)
    is the default until this is better understood. This has no effect
    on the LOCAL response cache below, which is always on regardless."""
    key = cache_mod.compute_cache_key(manifest, provider_name, model, params)
    lock = cache.lock(key)
    try:
        existing = cache.read(key)
        state = existing["validation.json"]["state"] if existing else None
        must_query = existing is None or state == validator_mod.STATE_API_FAILED or refresh or (
            retry_invalid and state not in (validator_mod.STATE_USABLE,)
        )
        if not must_query:
            return {"function": function_name, "cache_hit": True, "state": state, "key": key}

        shared_prefix_len = manifest.get("prompt_shared_prefix_bytes") if enable_prompt_caching else None
        prompt_cache_key = None
        if shared_prefix_len:
            prompt_cache_key = provider_mod.compute_prompt_cache_key(
                provider_name, model, manifest["prompt_version"], manifest["response_schema_version"],
                manifest["_prompt_text"][:shared_prefix_len],
            )
        try:
            result = provider_mod.query_with_retry(
                provider, manifest["_prompt_text"], function_name, model, params,
                max_attempts=max_attempts, timeout=timeout,
                shared_prefix_len=shared_prefix_len, prompt_cache_key=prompt_cache_key,
            )
        except provider_mod.ApiFailed as e:
            meta = _build_meta(function_name, manifest, provider_name, model, params)
            validation = {"state": validator_mod.STATE_API_FAILED, "errors": [e.reason]}
            usage = {"tokens": {}, "latency_s": None, "attempts": e.attempts,
                     "request_id": None, "completed_at": _now()}
            cache.write(key, meta, {}, "", validation, usage)
            return {"function": function_name, "cache_hit": False,
                    "state": validator_mod.STATE_API_FAILED, "key": key}

        vres = validator_mod.validate_response(result.extracted_text, function_name, manifest)
        meta = _build_meta(function_name, manifest, provider_name, model, params)
        usage = {
            "tokens": result.usage,
            "latency_s": result.latency_s,
            "attempts": result.attempts,
            "request_id": result.request_id,
            "completed_at": _now(),
        }
        cache.write(key, meta, result.raw_response, result.extracted_text, vres.to_dict(), usage)
        return {"function": function_name, "cache_hit": False, "state": vres.state, "key": key,
                "usage": result.usage}
    finally:
        lock.close()


def _build_meta(function_name, manifest, provider_name, model, params):
    return {
        "function": function_name,
        "prompt_hash": manifest["prompt_hash"],
        "input_hash": manifest["input_hash"],
        "prompt_version": manifest["prompt_version"],
        "response_schema_version": manifest["response_schema_version"],
        "provider": provider_name,
        "model": model,
        "params": params,
        "runner_request_format_version": cache_mod.RUNNER_REQUEST_FORMAT_VERSION,
        "requested_at": _now(),
    }


def parse_params(pairs):
    params = {}
    for p in pairs or []:
        k, _, v = p.partition("=")
        try:
            params[k] = json.loads(v)
        except json.JSONDecodeError:
            params[k] = v
    return params


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--prompt-dir", required=True)
    ap.add_argument("--cache-dir", required=True)
    ap.add_argument("--provider", required=True, choices=["openai", "fake"])
    ap.add_argument("--model", default=provider_mod.DEFAULT_OPENAI_MODEL,
                    help=f"default: {provider_mod.DEFAULT_OPENAI_MODEL}; for --provider openai, must be one of "
                         f"{sorted(provider_mod.SUPPORTED_OPENAI_MODELS)}")
    ap.add_argument("--fake-script", help="required when --provider fake")
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--max-attempts", type=int, default=5)
    ap.add_argument("--timeout", type=float, default=60.0)
    ap.add_argument("--summary", required=True)
    ap.add_argument("--function", action="append",
                    help="restrict to this function (repeatable); default is every eligible prompt in --prompt-dir")
    ap.add_argument("--param", action="append",
                    help="key=value request parameter (repeatable, e.g. --param temperature=0)")
    ap.add_argument("--refresh", action="store_true", help="re-query even a complete cache entry")
    ap.add_argument("--retry-invalid", action="store_true",
                    help="re-query entries that are not in the usable state")
    ap.add_argument("--enable-prompt-caching", action="store_true",
                    help="split each request into a system/user message pair with a prompt_cache_key, "
                         "for OpenAI's provider-side prompt caching. Off by default: an A/B test found "
                         "this splitting can change the model's answer on at least one real function, "
                         "so the default is the lower-risk single-message request shape. Unrelated to "
                         "this tool's own local response cache, which is always on.")
    args = ap.parse_args()

    if args.provider == "openai" and args.model not in provider_mod.SUPPORTED_OPENAI_MODELS:
        print(f"llm_query: --model {args.model!r} is not a supported OpenAI model "
              f"(supported: {sorted(provider_mod.SUPPORTED_OPENAI_MODELS)})", file=sys.stderr)
        return 1

    params = parse_params(args.param)
    if args.provider == "openai":
        reserved_used = provider_mod.RESERVED_OPENAI_PARAMS & set(params)
        if reserved_used:
            print(f"llm_query: --param cannot set reserved OpenAI request field(s): "
                  f"{sorted(reserved_used)}", file=sys.stderr)
            return 1

    if args.provider == "openai":
        api_key = os.environ.get("LLM_INFER_API_KEY")
        if not api_key:
            print("llm_query: LLM_INFER_API_KEY is not set", file=sys.stderr)
            return 1
        prov = provider_mod.OpenAIProvider(api_key)
    else:
        if not args.fake_script:
            print("llm_query: --provider fake requires --fake-script", file=sys.stderr)
            return 1
        prov = provider_mod.FakeProvider(args.fake_script)

    cache = cache_mod.Cache(args.cache_dir)

    discovered = list(discover(args.prompt_dir))
    if args.function:
        wanted = set(args.function)
        discovered = [d for d in discovered if d[0] in wanted]

    rejected = {}
    queryable = []
    for function_name, txt_path, json_path in discovered:
        manifest, reason = local_eligibility_check(txt_path, json_path)
        if reason is not None:
            rejected[reason] = rejected.get(reason, 0) + 1
            continue
        queryable.append((function_name, manifest))

    def bound_process_one(fn, manifest):
        return process_one(fn, manifest, cache, prov, args.provider, args.model,
                           params, args.max_attempts, args.timeout, args.refresh, args.retry_invalid,
                           enable_prompt_caching=args.enable_prompt_caching)

    # A batch of requests launched at the same instant races to establish
    # the provider's own prompt cache for their shared prefix -- none of
    # them can benefit from a cache that the batch itself hasn't warmed yet.
    # Running requests sequentially until one actually reaches the provider,
    # THEN starting the bounded-parallel pool for the rest, avoids that
    # self-inflicted cold-cache race. Only relevant when prompt caching
    # itself is enabled, and only openai has a provider-side prompt cache
    # to warm; the fake provider is deterministic test infrastructure, and
    # forcing its first request to run alone would defeat tests that
    # specifically exercise concurrent cache-lock behavior.
    warmup_enabled = args.enable_prompt_caching and args.provider == "openai" and args.jobs > 1
    results, remaining = dispatch_with_warmup(queryable, bound_process_one, warmup_enabled)

    with ThreadPoolExecutor(max_workers=max(1, args.jobs)) as pool:
        futs = {pool.submit(bound_process_one, fn, manifest): fn for fn, manifest in remaining}
        for fut in as_completed(futs):
            results.append(fut.result())

    by_state = {}
    total_usage = {}
    new_calls = 0
    cache_hits = 0
    requests_with_cached_tokens = 0
    for r in results:
        by_state[r["state"]] = by_state.get(r["state"], 0) + 1
        if r["cache_hit"]:
            cache_hits += 1
        else:
            new_calls += 1
            usage = r.get("usage") or {}
            for k, v in usage.items():
                if isinstance(v, (int, float)):
                    total_usage[k] = total_usage.get(k, 0) + v
            if usage.get("cached_input_tokens"):
                requests_with_cached_tokens += 1

    cached_input_tokens = total_usage.get("cached_input_tokens", 0)
    input_tokens = total_usage.get("input_tokens", 0)
    cache_effectiveness = {
        # Provider-side prompt caching (discounts the shared prefix of a
        # NEW API call) -- distinct from cache_hits above, which counts
        # calls this tool skipped entirely via its own local response cache.
        "cached_input_tokens": cached_input_tokens,
        "input_tokens": input_tokens,
        "cached_input_token_ratio": (cached_input_tokens / input_tokens) if input_tokens else None,
        "requests_with_cached_tokens": requests_with_cached_tokens,
        "completed_requests": new_calls,
        "requests_with_cached_tokens_ratio": (
            requests_with_cached_tokens / new_calls if new_calls else None
        ),
    }

    summary = {
        "provider": args.provider,
        "model": args.model,
        "params": params,
        "prompt_dir": os.path.abspath(args.prompt_dir),
        "discovered": len(discovered),
        "locally_rejected": rejected,
        "queryable": len(queryable),
        "cache_hits": cache_hits,
        "new_api_calls": new_calls,
        "counts_by_state": by_state,
        "total_usage_for_new_calls": total_usage,
        "cache_effectiveness": cache_effectiveness,
        "generated_at": _now(),
        "results": sorted(results, key=lambda r: r["function"]),
    }
    with open(args.summary, "w") as f:
        json.dump(summary, f, sort_keys=True, indent=2)

    print(f"llm_query: {len(queryable)} queryable, {cache_hits} cache hits, "
          f"{new_calls} new API calls, states={by_state}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
