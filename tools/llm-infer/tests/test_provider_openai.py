#!/usr/bin/env python3
"""Unit tests for OpenAIProvider -- HTTP status classification, usage-field
extraction, and secret redaction. Uses unittest.mock to stub urlopen: no
real network access, no real token required.
"""

import io
import json
import os
import sys
import unittest
import urllib.error
from unittest import mock

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import provider as p

FAKE_KEY = "sk-test-secret-do-not-leak-12345"


def _fake_http_response(body_dict):
    data = json.dumps(body_dict).encode("utf-8")
    cm = mock.MagicMock()
    cm.__enter__.return_value.read.return_value = data
    cm.__exit__.return_value = False
    return cm


class OpenAIProviderTests(unittest.TestCase):
    def test_successful_query_extracts_content_and_usage(self):
        body = {
            "id": "resp-123",
            "choices": [{"message": {"content": '{"ok": true}'}}],
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 20,
                "total_tokens": 120,
                "prompt_tokens_details": {"cached_tokens": 30},
                "completion_tokens_details": {"reasoning_tokens": 5},
            },
        }
        captured_request = {}

        def fake_urlopen(req, timeout=None):
            captured_request["req"] = req
            return _fake_http_response(body)

        with mock.patch("urllib.request.urlopen", side_effect=fake_urlopen):
            prov = p.OpenAIProvider(FAKE_KEY)
            result = prov.query_once("prompt text", "some_fn", "gpt-5.5", {}, timeout=5.0)

        self.assertEqual(result.extracted_text, '{"ok": true}')
        self.assertEqual(result.request_id, "resp-123")
        self.assertEqual(result.usage["input_tokens"], 100)
        self.assertEqual(result.usage["output_tokens"], 20)
        self.assertEqual(result.usage["total_tokens"], 120)
        self.assertEqual(result.usage["cached_input_tokens"], 30)
        self.assertEqual(result.usage["reasoning_tokens"], 5)

        # The key must appear in the OUTGOING request's Authorization header
        # (required for auth) but nowhere in anything the caller keeps.
        sent_req = captured_request["req"]
        self.assertIn(FAKE_KEY, sent_req.get_header("Authorization"))
        serialized_result = json.dumps({
            "raw_response": result.raw_response,
            "extracted_text": result.extracted_text,
            "usage": result.usage,
            "request_id": result.request_id,
        })
        self.assertNotIn(FAKE_KEY, serialized_result)

    def test_shared_prefix_splits_into_system_and_user_messages(self):
        body = {"id": "r1", "choices": [{"message": {"content": "{}"}}]}
        captured = {}

        def fake_urlopen(req, timeout=None):
            captured["body"] = json.loads(req.data.decode("utf-8"))
            return _fake_http_response(body)

        prompt_text = "SHARED PREAMBLE HERE" + "PER-FUNCTION BODY HERE"
        split = len("SHARED PREAMBLE HERE")
        with mock.patch("urllib.request.urlopen", side_effect=fake_urlopen):
            prov = p.OpenAIProvider(FAKE_KEY)
            prov.query_once(prompt_text, "fn", "gpt-5.5", {}, timeout=5.0, shared_prefix_len=split)

        self.assertEqual(captured["body"]["messages"], [
            {"role": "system", "content": "SHARED PREAMBLE HERE"},
            {"role": "user", "content": "PER-FUNCTION BODY HERE"},
        ])

    def test_no_shared_prefix_len_keeps_single_user_message(self):
        body = {"id": "r1", "choices": [{"message": {"content": "{}"}}]}
        captured = {}

        def fake_urlopen(req, timeout=None):
            captured["body"] = json.loads(req.data.decode("utf-8"))
            return _fake_http_response(body)

        with mock.patch("urllib.request.urlopen", side_effect=fake_urlopen):
            prov = p.OpenAIProvider(FAKE_KEY)
            prov.query_once("whole prompt, no split", "fn", "gpt-5.5", {}, timeout=5.0)

        self.assertEqual(captured["body"]["messages"], [
            {"role": "user", "content": "whole prompt, no split"},
        ])

    def test_shared_prefix_len_covering_whole_text_keeps_single_user_message(self):
        # A degenerate split point (>= the full text length) must not
        # produce an empty user message -- fall back to the ordinary shape.
        body = {"id": "r1", "choices": [{"message": {"content": "{}"}}]}
        captured = {}

        def fake_urlopen(req, timeout=None):
            captured["body"] = json.loads(req.data.decode("utf-8"))
            return _fake_http_response(body)

        with mock.patch("urllib.request.urlopen", side_effect=fake_urlopen):
            prov = p.OpenAIProvider(FAKE_KEY)
            prov.query_once("short", "fn", "gpt-5.5", {}, timeout=5.0, shared_prefix_len=999)

        self.assertEqual(captured["body"]["messages"], [{"role": "user", "content": "short"}])

    def test_prompt_cache_key_is_forwarded_when_given(self):
        body = {"id": "r1", "choices": [{"message": {"content": "{}"}}]}
        captured = {}

        def fake_urlopen(req, timeout=None):
            captured["body"] = json.loads(req.data.decode("utf-8"))
            return _fake_http_response(body)

        with mock.patch("urllib.request.urlopen", side_effect=fake_urlopen):
            prov = p.OpenAIProvider(FAKE_KEY)
            prov.query_once("p", "fn", "gpt-5.5", {}, timeout=5.0, prompt_cache_key="abc123")

        self.assertEqual(captured["body"]["prompt_cache_key"], "abc123")

    def test_prompt_cache_key_omitted_when_not_given(self):
        body = {"id": "r1", "choices": [{"message": {"content": "{}"}}]}
        captured = {}

        def fake_urlopen(req, timeout=None):
            captured["body"] = json.loads(req.data.decode("utf-8"))
            return _fake_http_response(body)

        with mock.patch("urllib.request.urlopen", side_effect=fake_urlopen):
            prov = p.OpenAIProvider(FAKE_KEY)
            prov.query_once("p", "fn", "gpt-5.5", {}, timeout=5.0)

        self.assertNotIn("prompt_cache_key", captured["body"])

    def test_extra_params_cannot_override_protected_fields(self):
        # A defense-in-depth backstop -- llm_query.py's CLI already rejects
        # these param names upfront, but query_once must stay safe even for
        # a caller that bypasses that check.
        body = {"id": "r1", "choices": [{"message": {"content": "{}"}}]}
        captured = {}

        def fake_urlopen(req, timeout=None):
            captured["body"] = json.loads(req.data.decode("utf-8"))
            return _fake_http_response(body)

        malicious_params = {
            "model": "some-other-model",
            "messages": [{"role": "user", "content": "hijacked"}],
            "response_format": {"type": "text"},
            "prompt_cache_key": "attacker-supplied",
            "temperature": 0,
        }
        with mock.patch("urllib.request.urlopen", side_effect=fake_urlopen):
            prov = p.OpenAIProvider(FAKE_KEY)
            prov.query_once("real prompt", "fn", "gpt-5.5", malicious_params, timeout=5.0)

        sent = captured["body"]
        self.assertEqual(sent["model"], "gpt-5.5")
        self.assertEqual(sent["messages"], [{"role": "user", "content": "real prompt"}])
        self.assertEqual(sent["response_format"], {"type": "json_schema", "json_schema": p.RESPONSE_SCHEMA_JSON_SCHEMA})
        self.assertNotIn("prompt_cache_key", sent)
        self.assertEqual(sent["temperature"], 0)  # a non-reserved param still passes through

    def test_absent_usage_fields_are_omitted_not_invented(self):
        body = {"id": "r1", "choices": [{"message": {"content": "{}"}}]}  # no "usage" key at all
        with mock.patch("urllib.request.urlopen", return_value=_fake_http_response(body)):
            prov = p.OpenAIProvider(FAKE_KEY)
            result = prov.query_once("p", "fn", "gpt-5.5", {}, timeout=5.0)
        self.assertEqual(result.usage, {})

    def _http_error(self, status, body_text="error detail"):
        return urllib.error.HTTPError(
            url="https://api.openai.com/v1/chat/completions", code=status, msg="err",
            hdrs=None, fp=io.BytesIO(body_text.encode("utf-8")),
        )

    def test_429_is_transient(self):
        with mock.patch("urllib.request.urlopen", side_effect=self._http_error(429)):
            prov = p.OpenAIProvider(FAKE_KEY)
            with self.assertRaises(p.TransientError):
                prov.query_once("p", "fn", "gpt-5.5", {}, timeout=5.0)

    def test_500_is_transient(self):
        with mock.patch("urllib.request.urlopen", side_effect=self._http_error(500)):
            prov = p.OpenAIProvider(FAKE_KEY)
            with self.assertRaises(p.TransientError):
                prov.query_once("p", "fn", "gpt-5.5", {}, timeout=5.0)

    def test_400_is_permanent(self):
        with mock.patch("urllib.request.urlopen", side_effect=self._http_error(400)):
            prov = p.OpenAIProvider(FAKE_KEY)
            with self.assertRaises(p.PermanentError):
                prov.query_once("p", "fn", "gpt-5.5", {}, timeout=5.0)

    def test_401_is_permanent(self):
        with mock.patch("urllib.request.urlopen", side_effect=self._http_error(401)):
            prov = p.OpenAIProvider(FAKE_KEY)
            with self.assertRaises(p.PermanentError):
                prov.query_once("p", "fn", "gpt-5.5", {}, timeout=5.0)

    def test_connection_error_is_transient(self):
        with mock.patch("urllib.request.urlopen", side_effect=urllib.error.URLError("connection refused")):
            prov = p.OpenAIProvider(FAKE_KEY)
            with self.assertRaises(p.TransientError):
                prov.query_once("p", "fn", "gpt-5.5", {}, timeout=5.0)


class SupportedModelsTests(unittest.TestCase):
    def test_default_model_is_itself_supported(self):
        self.assertIn(p.DEFAULT_OPENAI_MODEL, p.SUPPORTED_OPENAI_MODELS)


class PromptCacheKeyTests(unittest.TestCase):
    def test_same_inputs_produce_same_key(self):
        k1 = p.compute_prompt_cache_key("openai", "gpt-5.5", "marshal-ir-v2", "marshal-response-v6", "shared prefix")
        k2 = p.compute_prompt_cache_key("openai", "gpt-5.5", "marshal-ir-v2", "marshal-response-v6", "shared prefix")
        self.assertEqual(k1, k2)

    def test_different_shared_prefix_produces_different_key(self):
        k1 = p.compute_prompt_cache_key("openai", "gpt-5.5", "marshal-ir-v2", "marshal-response-v6", "prefix A")
        k2 = p.compute_prompt_cache_key("openai", "gpt-5.5", "marshal-ir-v2", "marshal-response-v6", "prefix B")
        self.assertNotEqual(k1, k2)

    def test_different_model_produces_different_key(self):
        k1 = p.compute_prompt_cache_key("openai", "gpt-5.5", "marshal-ir-v2", "marshal-response-v6", "shared prefix")
        k2 = p.compute_prompt_cache_key("openai", "gpt-6-sol", "marshal-ir-v2", "marshal-response-v6", "shared prefix")
        self.assertNotEqual(k1, k2)


class RetryLogicTests(unittest.TestCase):
    def test_transient_then_success_retries_and_returns(self):
        calls = {"n": 0}

        class FlakyProvider:
            def query_once(self, prompt, fn, model, params, timeout, shared_prefix_len=None, prompt_cache_key=None):
                calls["n"] += 1
                if calls["n"] < 3:
                    raise p.TransientError("flaky")
                return p.QueryResult(raw_response={}, extracted_text="ok", usage={}, request_id="r", latency_s=0.0)

        result = p.query_with_retry(FlakyProvider(), "p", "fn", "m", {}, max_attempts=5, sleep=lambda s: None)
        self.assertEqual(result.attempts, 3)
        self.assertEqual(calls["n"], 3)

    def test_permanent_error_is_not_retried(self):
        calls = {"n": 0}

        class AlwaysPermanent:
            def query_once(self, prompt, fn, model, params, timeout, shared_prefix_len=None, prompt_cache_key=None):
                calls["n"] += 1
                raise p.PermanentError("nope")

        with self.assertRaises(p.ApiFailed):
            p.query_with_retry(AlwaysPermanent(), "p", "fn", "m", {}, max_attempts=5, sleep=lambda s: None)
        self.assertEqual(calls["n"], 1)

    def test_transient_exhausts_max_attempts(self):
        calls = {"n": 0}

        class AlwaysTransient:
            def query_once(self, prompt, fn, model, params, timeout, shared_prefix_len=None, prompt_cache_key=None):
                calls["n"] += 1
                raise p.TransientError("still down")

        with self.assertRaises(p.ApiFailed) as cm:
            p.query_with_retry(AlwaysTransient(), "p", "fn", "m", {}, max_attempts=3, sleep=lambda s: None)
        self.assertEqual(calls["n"], 3)
        self.assertEqual(cm.exception.attempts, 3)


if __name__ == "__main__":
    unittest.main()
