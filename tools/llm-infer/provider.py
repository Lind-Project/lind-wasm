"""Provider-neutral request/result interface. See
local-notes/active/plan-llm-api-inference.md section 5. Cache, validation,
and reporting code depend only on QueryResult and the two exception types
below -- never on a specific provider's response envelope shape, so adding
a second real provider later never touches that code.
"""

import hashlib
import json
import time
import urllib.error
import urllib.request


# Request fields OpenAIProvider itself controls -- a --param can never be
# allowed to override these, since doing so could disable strict-mode
# output or break the cache-prefix identity while the cache key and run
# summary would still describe the ORIGINAL (uncorrupted) request. Checked
# by the CLI (llm_query.py's main, for a clear upfront error) and enforced
# again here (see query_once) as a non-bypassable last line of defense.
RESERVED_OPENAI_PARAMS = frozenset({"model", "messages", "response_format", "prompt_cache_key"})

# The models this pipeline has actually been exercised against (see
# local-notes/active/plan-llm-marshal-inference.md) -- not every model name
# OpenAI happens to expose to this account. DEFAULT_OPENAI_MODEL is used
# when --model is omitted; both names must also appear in this set.
DEFAULT_OPENAI_MODEL = "gpt-6-sol"
SUPPORTED_OPENAI_MODELS = frozenset({"gpt-5.6-sol", "gpt-6-sol"})


def compute_prompt_cache_key(provider_name, model, prompt_version, response_schema_version, shared_prefix_text):
    """A stable key identifying "requests that share the same cacheable
    prefix", passed to OpenAI's `prompt_cache_key` request field so it can
    group a burst of concurrent requests onto the same cache partition
    instead of leaving that grouping to chance. Deliberately excludes the
    function name and the full prompt hash -- every function at a given
    (provider, model, prompt_version, response_schema_version) shares the
    identical shared_prefix_text, so they must all hash to the SAME key for
    this to do anything; keying on anything function-specific would defeat
    the purpose."""
    digest_input = "|".join([
        provider_name, model, prompt_version, response_schema_version,
        hashlib.sha256(shared_prefix_text.encode("utf-8")).hexdigest(),
    ])
    return hashlib.sha256(digest_input.encode("utf-8")).hexdigest()

# The single canonical description of a marshal-response-v6 answer, shared
# by the OpenAI request (below), validator.py, compare_static.py, and the
# tests -- there is exactly one contract, defined once. OpenAI strict mode
# requires every object to close with additionalProperties: false and every
# property to be listed in "required" (a property that is logically
# optional is instead typed as a nullable union and set to null).
#
# An operand slot is recursive: a leaf ({"source": ...}) or a composite
# ({"op": "product"|"abs"|"max"|"add"|"divide", ...}) wrapping further
# operand(s). $defs/$ref expresses that recursion; MAX_OPERAND_DEPTH in
# validator.py is enforced separately, since JSON Schema alone cannot bound
# recursion depth.
_OPERAND_LEAF = {
    "type": "object",
    "additionalProperties": False,
    "properties": {
        "source": {"type": "string", "enum": ["argument_value", "argument_pointee", "constant"]},
        "argument_id": {"type": ["string", "null"]},
        "constant_value": {"type": ["integer", "null"]},
    },
    "required": ["source", "argument_id", "constant_value"],
}
_OPERAND_PRODUCT = {
    "type": "object",
    "additionalProperties": False,
    "properties": {
        "op": {"type": "string", "enum": ["product"]},
        "operands": {"type": "array", "items": {"$ref": "#/$defs/operand"}, "minItems": 2, "maxItems": 2},
    },
    "required": ["op", "operands"],
}
_OPERAND_ABS = {
    "type": "object",
    "additionalProperties": False,
    "properties": {
        "op": {"type": "string", "enum": ["abs"]},
        "operand": {"$ref": "#/$defs/operand"},
    },
    "required": ["op", "operand"],
}
_OPERAND_MAX = {
    "type": "object",
    "additionalProperties": False,
    "properties": {
        "op": {"type": "string", "enum": ["max"]},
        "operands": {"type": "array", "items": {"$ref": "#/$defs/operand"}, "minItems": 2, "maxItems": 2},
    },
    "required": ["op", "operands"],
}
_OPERAND_ADD = {
    "type": "object",
    "additionalProperties": False,
    "properties": {
        "op": {"type": "string", "enum": ["add"]},
        "operands": {"type": "array", "items": {"$ref": "#/$defs/operand"}, "minItems": 2, "maxItems": 2},
    },
    "required": ["op", "operands"],
}
# operands[0] is the dividend, operands[1] the divisor; the runtime always
# performs CEILING division (see validator.py's COMPOSITE_OPS comment).
_OPERAND_DIVIDE = {
    "type": "object",
    "additionalProperties": False,
    "properties": {
        "op": {"type": "string", "enum": ["divide"]},
        "operands": {"type": "array", "items": {"$ref": "#/$defs/operand"}, "minItems": 2, "maxItems": 2},
    },
    "required": ["op", "operands"],
}
_SIZE_ONLY_OPERAND = {
    "type": "object",
    "additionalProperties": False,
    "properties": {"size": {"$ref": "#/$defs/operand"}},
    "required": ["size"],
}
_STRIDE_VECTOR_OPERAND = {
    "type": "object",
    "additionalProperties": False,
    "properties": {
        "size": {"$ref": "#/$defs/operand"},
        "stride": {"$ref": "#/$defs/operand"},
    },
    "required": ["size", "stride"],
}

RESPONSE_SCHEMA_JSON_SCHEMA = {
    "name": "marshal_response_v6",
    "strict": True,
    "schema": {
        "type": "object",
        "additionalProperties": False,
        "$defs": {
            # OpenAI's structured-outputs strict mode rejects "oneOf" outright
            # ("'oneOf' is not permitted") -- only "anyOf" is accepted, even
            # though these six shapes are in fact mutually exclusive.
            "operand": {"anyOf": [
                _OPERAND_LEAF, _OPERAND_PRODUCT, _OPERAND_ABS, _OPERAND_MAX, _OPERAND_ADD, _OPERAND_DIVIDE,
            ]},
        },
        "properties": {
            "response_schema_version": {"type": "string", "enum": ["marshal-response-v6"]},
            "function": {"type": "string"},
            "pointer_arguments": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": False,
                    "properties": {
                        "id": {"type": "string"},
                        "direction": {"type": "string", "enum": ["in", "out", "inout", "unknown"]},
                        "extent": {
                            "type": "string",
                            "enum": ["one", "constant", "argument", "c_string", "stride_vector", "unknown"],
                        },
                        # The shape of extent_operand depends on "extent" (enforced by
                        # validator.py, which -- unlike this schema -- can express that
                        # conditional relationship precisely); this just admits every
                        # shape any valid extent could carry, including null for the
                        # three extents that carry none at all.
                        "extent_operand": {
                            "anyOf": [_SIZE_ONLY_OPERAND, _STRIDE_VECTOR_OPERAND, {"type": "null"}],
                        },
                    },
                    "required": ["id", "direction", "extent", "extent_operand"],
                },
            },
        },
        "required": ["response_schema_version", "function", "pointer_arguments"],
    },
}


class TransientError(Exception):
    """A retryable failure: connection error, timeout, HTTP 429, HTTP 5xx."""


class PermanentError(Exception):
    """A non-retryable transport/auth failure (HTTP 4xx other than 429, or a
    provider the runner has no idea how to interpret at all)."""


class ApiFailed(Exception):
    """Raised by query_with_retry once max_attempts is exhausted (or a
    PermanentError is hit) -- the terminal `api_failed` state (section 6)."""

    def __init__(self, reason, attempts):
        super().__init__(reason)
        self.reason = reason
        self.attempts = attempts


class QueryResult:
    def __init__(self, raw_response, extracted_text, usage, request_id, latency_s, attempts=None):
        self.raw_response = raw_response
        self.extracted_text = extracted_text
        self.usage = usage  # dict; only provider-reported fields, never an invented 0 for an absent one
        self.request_id = request_id
        self.latency_s = latency_s
        self.attempts = attempts


def query_with_retry(provider, prompt_text, function_name, model, params,
                     max_attempts=5, timeout=60.0, backoff_base=1.0, sleep=time.sleep,
                     shared_prefix_len=None, prompt_cache_key=None):
    """Bounded exponential backoff for TRANSPORT failures only. A
    PermanentError (bad auth, malformed request) is never retried. A
    successful transport response that turns out to be semantically invalid
    JSON/schema is NOT a TransientError at all -- query_once always returns
    a QueryResult for that case, letting the caller's validator assign
    json_invalid/schema_invalid instead of this function ever seeing it.

    `shared_prefix_len`, when given, is the byte length of prompt_text's
    leading span that is identical across every function at this prompt
    version (marshal-infer's own `prompt_shared_prefix_bytes` manifest
    field) -- passed through so a provider that benefits from a stable,
    repeated prefix (e.g. OpenAI's automatic prompt caching, which discounts
    a request's leading tokens when they match a recently-sent request) can
    place it in its own cacheable slot instead of resending it as
    undifferentiated input every time.

    `prompt_cache_key`, when given, is forwarded to the provider so a whole
    batch of otherwise-unrelated requests that share the same prefix can be
    steered onto the same cache partition -- see compute_prompt_cache_key."""
    attempt = 0
    last_reason = "no attempt made"
    while attempt < max_attempts:
        attempt += 1
        try:
            result = provider.query_once(prompt_text, function_name, model, params, timeout,
                                         shared_prefix_len=shared_prefix_len,
                                         prompt_cache_key=prompt_cache_key)
            result.attempts = attempt
            return result
        except TransientError as e:
            last_reason = str(e)
            if attempt >= max_attempts:
                break
            sleep(backoff_base * (2 ** (attempt - 1)))
        except PermanentError as e:
            last_reason = str(e)
            break
    raise ApiFailed(last_reason, attempts=attempt)


class OpenAIProvider:
    API_URL = "https://api.openai.com/v1/chat/completions"

    def __init__(self, api_key):
        self._api_key = api_key  # never placed on self in any attribute a caller could serialize/log

    def query_once(self, prompt_text, function_name, model, params, timeout,
                   shared_prefix_len=None, prompt_cache_key=None):
        # A stable, byte-identical-across-queries prefix is split into its
        # own leading message so it lines up with OpenAI's automatic prompt
        # caching, which discounts a request's leading tokens when they
        # match a recently-sent request -- caching keys off the literal
        # token prefix of the whole request, so this prefix must stay
        # FIRST and byte-for-byte identical every time for it to hit.
        if shared_prefix_len and 0 < shared_prefix_len < len(prompt_text):
            messages = [
                {"role": "system", "content": prompt_text[:shared_prefix_len]},
                {"role": "user", "content": prompt_text[shared_prefix_len:]},
            ]
        else:
            messages = [{"role": "user", "content": prompt_text}]
        # Extra request parameters are applied FIRST, so the four
        # RESERVED_OPENAI_PARAMS set below always win regardless of what a
        # caller passed in `params` -- the CLI rejects those names upfront
        # (a clear error beats a silently-overridden request), but this
        # ordering keeps query_once itself safe for any other caller too.
        body = dict(params)
        body["model"] = model
        body["messages"] = messages
        body["response_format"] = {"type": "json_schema", "json_schema": RESPONSE_SCHEMA_JSON_SCHEMA}
        if prompt_cache_key:
            body["prompt_cache_key"] = prompt_cache_key
        else:
            body.pop("prompt_cache_key", None)
        data = json.dumps(body).encode("utf-8")
        req = urllib.request.Request(
            self.API_URL,
            data=data,
            method="POST",
            headers={"Authorization": f"Bearer {self._api_key}", "Content-Type": "application/json"},
        )
        t0 = time.monotonic()
        try:
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                raw = json.loads(resp.read().decode("utf-8"))
        except urllib.error.HTTPError as e:
            status = e.code
            detail = e.read().decode("utf-8", errors="replace")[:500]
            if status == 429 or status >= 500:
                raise TransientError(f"HTTP {status}: {detail}")
            raise PermanentError(f"HTTP {status}: {detail}")
        except (urllib.error.URLError, TimeoutError, OSError) as e:
            raise TransientError(str(e))
        latency_s = time.monotonic() - t0

        try:
            extracted = raw["choices"][0]["message"]["content"] or ""
        except (KeyError, IndexError, TypeError):
            extracted = ""

        usage_raw = raw.get("usage") or {}
        usage = {}
        for src, dst in (("prompt_tokens", "input_tokens"),
                        ("completion_tokens", "output_tokens"),
                        ("total_tokens", "total_tokens")):
            if src in usage_raw:
                usage[dst] = usage_raw[src]
        if "cached_tokens" in (usage_raw.get("prompt_tokens_details") or {}):
            usage["cached_input_tokens"] = usage_raw["prompt_tokens_details"]["cached_tokens"]
        if "reasoning_tokens" in (usage_raw.get("completion_tokens_details") or {}):
            usage["reasoning_tokens"] = usage_raw["completion_tokens_details"]["reasoning_tokens"]

        return QueryResult(
            raw_response=raw,
            extracted_text=extracted,
            usage=usage,
            request_id=raw.get("id"),
            latency_s=latency_s,
        )


class FakeProvider:
    """Deterministic provider for tests -- no network, no credentials.
    `script_path` is a JSON file: {function_name: [step, step, ...]}. Each
    call for a given function consumes the next step in its list (the last
    step repeats for any call beyond the scripted sequence, so a test can
    script "fail twice then succeed" without knowing exactly how many
    retries query_with_retry will attempt). A step is one of:
      {"kind": "response", "response": <dict>, "usage": {...}?, "request_id": "..."?}
      {"kind": "response", "text": "<raw text, e.g. malformed/prose-wrapped>", ...}
      {"kind": "transient_error", "message": "..."?}
      {"kind": "permanent_error", "message": "..."?}
    Any step may also carry "delay_s": <float> -- held before returning/
    raising, used by tests to make a lock race between two threads
    deterministic instead of relying on scheduling luck.
    """

    def __init__(self, script_path):
        with open(script_path) as f:
            self._script = json.load(f)
        self._call_counts = {}

    def query_once(self, prompt_text, function_name, model, params, timeout,
                   shared_prefix_len=None, prompt_cache_key=None):
        steps = self._script.get(function_name)
        if not steps:
            raise PermanentError(f"fake provider script has no entry for {function_name!r}")
        idx = self._call_counts.get(function_name, 0)
        self._call_counts[function_name] = idx + 1
        step = steps[min(idx, len(steps) - 1)]

        if step.get("delay_s"):
            time.sleep(step["delay_s"])

        kind = step["kind"]
        if kind == "transient_error":
            raise TransientError(step.get("message", "fake transient error"))
        if kind == "permanent_error":
            raise PermanentError(step.get("message", "fake permanent error"))

        text = step["text"] if "text" in step else json.dumps(step["response"])
        return QueryResult(
            raw_response={"fake_provider_step": step, "call_index": idx},
            extracted_text=text,
            usage=step.get("usage", {}),
            request_id=step.get("request_id", f"fake-{function_name}-{idx}"),
            latency_s=0.0,
        )
