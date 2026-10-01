#!/usr/bin/env python3
"""Unit tests for llm_query.py -- dispatch_with_warmup's local-cache-hit-
skipping contract, and process_one's enable_prompt_caching gate.
"""

import json
import os
import shutil
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import cache as cache_mod
import llm_query as lq
import provider as provider_mod


def _manifest(fn):
    return {"function": fn}


class DispatchWithWarmupTests(unittest.TestCase):
    def test_disabled_runs_nothing_and_keeps_full_list(self):
        queryable = [("fn_a", _manifest("fn_a")), ("fn_b", _manifest("fn_b"))]
        calls = []

        def process_one_fn(fn, manifest):
            calls.append(fn)
            return {"function": fn, "cache_hit": False}

        results, remaining = lq.dispatch_with_warmup(queryable, process_one_fn, warmup_enabled=False)
        self.assertEqual(results, [])
        self.assertEqual(remaining, queryable)
        self.assertEqual(calls, [])

    def test_single_function_skips_warmup_even_when_enabled(self):
        queryable = [("fn_a", _manifest("fn_a"))]
        calls = []

        def process_one_fn(fn, manifest):
            calls.append(fn)
            return {"function": fn, "cache_hit": False}

        results, remaining = lq.dispatch_with_warmup(queryable, process_one_fn, warmup_enabled=True)
        self.assertEqual(results, [])
        self.assertEqual(remaining, queryable)
        self.assertEqual(calls, [])

    def test_first_function_is_a_real_request_stops_after_one(self):
        queryable = [("fn_a", _manifest("fn_a")), ("fn_b", _manifest("fn_b")), ("fn_c", _manifest("fn_c"))]
        calls = []

        def process_one_fn(fn, manifest):
            calls.append(fn)
            return {"function": fn, "cache_hit": False}

        results, remaining = lq.dispatch_with_warmup(queryable, process_one_fn, warmup_enabled=True)
        self.assertEqual(calls, ["fn_a"])
        self.assertEqual([r["function"] for r in results], ["fn_a"])
        self.assertEqual(remaining, queryable[1:])

    def test_leading_cache_hits_are_skipped_until_a_real_request_lands(self):
        # fn_a and fn_b are already cached locally (no provider request is
        # made for either) -- warm-up must keep walking forward instead of
        # treating fn_a's no-op cache hit as having warmed anything.
        queryable = [("fn_a", _manifest("fn_a")), ("fn_b", _manifest("fn_b")),
                    ("fn_c", _manifest("fn_c")), ("fn_d", _manifest("fn_d"))]
        calls = []

        def process_one_fn(fn, manifest):
            calls.append(fn)
            return {"function": fn, "cache_hit": fn in ("fn_a", "fn_b")}

        results, remaining = lq.dispatch_with_warmup(queryable, process_one_fn, warmup_enabled=True)
        self.assertEqual(calls, ["fn_a", "fn_b", "fn_c"])
        self.assertEqual([r["function"] for r in results], ["fn_a", "fn_b", "fn_c"])
        self.assertEqual(remaining, queryable[3:])

    def test_all_cache_hits_consumes_everything_with_nothing_remaining(self):
        queryable = [("fn_a", _manifest("fn_a")), ("fn_b", _manifest("fn_b"))]
        calls = []

        def process_one_fn(fn, manifest):
            calls.append(fn)
            return {"function": fn, "cache_hit": True}

        results, remaining = lq.dispatch_with_warmup(queryable, process_one_fn, warmup_enabled=True)
        self.assertEqual(calls, ["fn_a", "fn_b"])
        self.assertEqual([r["function"] for r in results], ["fn_a", "fn_b"])
        self.assertEqual(remaining, [])


class _CapturingProvider:
    """Records the shared_prefix_len/prompt_cache_key it was called with and
    returns a trivially-valid (no pointer arguments) usable response."""

    def __init__(self):
        self.calls = []

    def query_once(self, prompt_text, function_name, model, params, timeout,
                   shared_prefix_len=None, prompt_cache_key=None):
        self.calls.append({"shared_prefix_len": shared_prefix_len, "prompt_cache_key": prompt_cache_key})
        response = {"response_schema_version": "marshal-response-v6", "function": function_name,
                    "pointer_arguments": []}
        return provider_mod.QueryResult(raw_response={}, extracted_text=json.dumps(response),
                                        usage={}, request_id="r", latency_s=0.0)


class ProcessOneEnablePromptCachingTests(unittest.TestCase):
    def setUp(self):
        self.tmpdir = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.tmpdir, ignore_errors=True)
        self.cache = cache_mod.Cache(self.tmpdir)
        prompt_text = "SHARED PREFIX" + "PER-FUNCTION BODY"
        self.manifest = {
            "function": "fn1",
            "prompt_hash": "sha256:" + "0" * 64,
            "input_hash": "sha256:" + "1" * 64,
            "prompt_version": "marshal-ir-v2",
            "response_schema_version": "marshal-response-v6",
            "entry_arguments": [],
            "prompt_shared_prefix_bytes": len("SHARED PREFIX"),
            "_prompt_text": prompt_text,
        }

    def test_disabled_by_default_sends_no_shared_prefix_or_cache_key(self):
        prov = _CapturingProvider()
        lq.process_one("fn1", self.manifest, self.cache, prov, "openai", "gpt-6-sol",
                       {}, max_attempts=1, timeout=5.0, refresh=False, retry_invalid=False)
        self.assertEqual(prov.calls, [{"shared_prefix_len": None, "prompt_cache_key": None}])

    def test_enabled_sends_shared_prefix_and_cache_key(self):
        prov = _CapturingProvider()
        lq.process_one("fn1", self.manifest, self.cache, prov, "openai", "gpt-6-sol",
                       {}, max_attempts=1, timeout=5.0, refresh=False, retry_invalid=False,
                       enable_prompt_caching=True)
        self.assertEqual(len(prov.calls), 1)
        self.assertEqual(prov.calls[0]["shared_prefix_len"], len("SHARED PREFIX"))
        self.assertIsNotNone(prov.calls[0]["prompt_cache_key"])


if __name__ == "__main__":
    unittest.main()
