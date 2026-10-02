#!/usr/bin/env python3
"""Unit tests for build_final_inference.py's candidate grouping/selection:
agreement, disagreement (conflict detection), stale-IR rejection, and
corrupt-cache-entry handling. See the regression this guards against:
an earlier version baked the (unique) cache key into the semantic rank
itself, so `tied` could never contain more than one candidate and
conflicting answers were never detected.
"""

import json
import os
import shutil
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import build_final_inference as bfi
import cache as cache_mod
import validator as validator_mod

MANIFEST = {
    "prompt_version": "marshal-ir-v2",
    "response_schema_version": "marshal-response-v6",
    "slice_complete": True,
    "eligible_for_llm_inference": True,
    "prompt_hash": "sha256:" + "a" * 64,
    "input_hash": "sha256:" + "b" * 64,
    "entry_arguments": [{"id": "arg0", "name": "x", "llvm_type": "ptr"}],
}


def _response_text(function_name, state, value=1):
    if state == "usable":
        pointer_arguments = [{
            "id": "arg0", "direction": "in", "extent": "constant",
            "extent_operand": {"size": {"source": "constant", "argument_id": None, "constant_value": value}},
        }]
    else:  # "model_unknown"
        pointer_arguments = [{"id": "arg0", "direction": "unknown", "extent": "unknown", "extent_operand": None}]
    return json.dumps({"response_schema_version": "marshal-response-v6",
                       "function": function_name, "pointer_arguments": pointer_arguments})


class BuildFinalInferenceTests(unittest.TestCase):
    def setUp(self):
        self.tmpdir = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.tmpdir, ignore_errors=True)
        self.cache_dir = os.path.join(self.tmpdir, "cache")
        self.prompt_dir = os.path.join(self.tmpdir, "prompts")
        self.raw_dir = os.path.join(self.tmpdir, "raw")
        os.makedirs(self.prompt_dir)
        self.cache = cache_mod.Cache(self.cache_dir)

    def _write_manifest(self, function_name, **overrides):
        manifest = dict(MANIFEST, **overrides)
        with open(os.path.join(self.prompt_dir, f"{function_name}.prompt.json"), "w") as f:
            json.dump(manifest, f)
        return manifest

    def _write_entry(self, function_name, provider, model, state="usable", value=1,
                     prompt_hash=None, input_hash=None, params=None):
        manifest = dict(MANIFEST, prompt_hash=prompt_hash or MANIFEST["prompt_hash"],
                       input_hash=input_hash or MANIFEST["input_hash"])
        params = params or {}
        key = cache_mod.compute_cache_key(manifest, provider, model, params)
        meta = {
            "function": function_name, "prompt_hash": manifest["prompt_hash"],
            "input_hash": manifest["input_hash"], "prompt_version": manifest["prompt_version"],
            "response_schema_version": manifest["response_schema_version"],
            "provider": provider, "model": model, "params": params,
            "runner_request_format_version": cache_mod.RUNNER_REQUEST_FORMAT_VERSION,
            "requested_at": "2026-01-01T00:00:00+00:00",
        }
        text = _response_text(function_name, state, value)
        vres = validator_mod.validate_response(text, function_name, manifest)
        usage = {"tokens": {}, "latency_s": 0.0, "attempts": 1, "request_id": "r",
                "completed_at": meta["requested_at"]}
        self.cache.write(key, meta, {}, text, vres.to_dict(), usage)
        return key

    def test_agreement_same_rank_same_answer_is_not_a_conflict(self):
        self._write_manifest("fn_agree")
        self._write_entry("fn_agree", "openai", "gpt-6-sol", value=1)
        self._write_entry("fn_agree", "openai", "gpt-5.5", value=1)
        results, _ = bfi.build_llm_results(self.raw_dir, cache_dir=self.cache_dir, prompt_dir=self.prompt_dir)
        row = results["fn_agree"]
        self.assertEqual(row["state"], "usable")
        self.assertEqual(row["candidates_considered"], 2)
        self.assertNotIn("conflicting_candidates", row)
        # the better-ranked model wins deterministically
        self.assertEqual(row["model"], "gpt-6-sol")

    def test_disagreement_same_rank_different_answer_is_flagged_as_conflict(self):
        self._write_manifest("fn_disagree")
        # same model -- would collide on cache key unless params differ,
        # so use distinct params to get two real, distinct entries while
        # keeping them at the identical semantic rank (same model).
        self._write_entry("fn_disagree", "openai", "gpt-6-sol", value=1, params={})
        self._write_entry("fn_disagree", "openai", "gpt-6-sol", value=2, params={"seed": 2})
        results, _ = bfi.build_llm_results(self.raw_dir, cache_dir=self.cache_dir, prompt_dir=self.prompt_dir)
        row = results["fn_disagree"]
        self.assertIn("conflicting_candidates", row)
        self.assertEqual(len(row["conflicting_candidates"]), 2)

    def test_disagreement_propagates_to_flagged_not_resolved_in_final(self):
        self._write_manifest("fn_disagree2")
        self._write_entry("fn_disagree2", "openai", "gpt-6-sol", value=1, params={})
        self._write_entry("fn_disagree2", "openai", "gpt-6-sol", value=2, params={"seed": 2})
        results, _ = bfi.build_llm_results(self.raw_dir, cache_dir=self.cache_dir, prompt_dir=self.prompt_dir)
        static_functions = {"fn_disagree2": {"name": "fn_disagree2", "decision": "force_local"}}
        final = bfi.build_final(static_functions, results)
        self.assertEqual(final["fn_disagree2"]["status"], "flagged")
        self.assertIn("conflicting_candidates", final["fn_disagree2"])

    def test_different_rank_candidates_never_conflict(self):
        # One usable, one model_unknown -- not a tie, so never a conflict
        # even though there are 2 candidates.
        self._write_manifest("fn_mixed")
        self._write_entry("fn_mixed", "openai", "gpt-6-sol", state="usable", value=1)
        self._write_entry("fn_mixed", "openai", "gpt-5.5", state="model_unknown")
        results, _ = bfi.build_llm_results(self.raw_dir, cache_dir=self.cache_dir, prompt_dir=self.prompt_dir)
        row = results["fn_mixed"]
        self.assertEqual(row["state"], "usable")
        self.assertNotIn("conflicting_candidates", row)

    def test_stale_input_hash_mismatch_is_rejected(self):
        self._write_manifest("fn_stale")
        self._write_entry("fn_stale", "openai", "gpt-6-sol", value=1,
                          input_hash="sha256:" + "c" * 64)  # doesn't match the manifest written above
        results, stats = bfi.build_llm_results(self.raw_dir, cache_dir=self.cache_dir, prompt_dir=self.prompt_dir)
        self.assertEqual(results["fn_stale"]["state"], "not_queried")
        self.assertEqual(stats["stale_ir_entries_skipped"], 1)

    def test_stale_prompt_hash_mismatch_is_rejected(self):
        self._write_manifest("fn_stale2")
        self._write_entry("fn_stale2", "openai", "gpt-6-sol", value=1,
                          prompt_hash="sha256:" + "d" * 64)
        results, stats = bfi.build_llm_results(self.raw_dir, cache_dir=self.cache_dir, prompt_dir=self.prompt_dir)
        self.assertEqual(results["fn_stale2"]["state"], "not_queried")
        self.assertEqual(stats["stale_ir_entries_skipped"], 1)

    def test_corrupt_cache_entry_is_skipped_not_trusted(self):
        self._write_manifest("fn_corrupt")
        key = self._write_entry("fn_corrupt", "openai", "gpt-6-sol", value=1)
        entry_dir = os.path.join(self.cache_dir, key[:2], key)
        # tamper with a file after writing, so its hash no longer matches
        # the integrity.json recorded at write time
        with open(os.path.join(entry_dir, "extracted.txt"), "w") as f:
            f.write("TAMPERED, DOES NOT MATCH INTEGRITY HASH")
        results, stats = bfi.build_llm_results(self.raw_dir, cache_dir=self.cache_dir, prompt_dir=self.prompt_dir)
        self.assertEqual(results["fn_corrupt"]["state"], "not_queried")
        self.assertEqual(stats["corrupt_entries_skipped"], 1)


if __name__ == "__main__":
    unittest.main()
