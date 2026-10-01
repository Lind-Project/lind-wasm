"""Content-addressed cache for LLM query attempts. See
local-notes/active/plan-llm-api-inference.md section 4.

A cache entry is reusable only when every response-affecting input agrees:
the manifest's prompt_hash/input_hash, the prompt/response-schema versions,
the provider, the exact model, the sampling/reasoning parameters, and this
runner's own request-format version. The key is a SHA256 of a canonical
encoding of exactly those fields -- never the function name alone, never a
timestamp, never anything that would make two runs against IDENTICAL inputs
produce different keys.

Concurrency contract: a caller must hold lock(key) for the entire
read-decide-write sequence on that key. Cache.write() itself stages a
complete entry in a temporary directory and renames it into place, so a
reader that is NOT holding the lock (there should be none, by convention)
can still never observe a partially-written entry -- only ever "absent" or
"complete for some prior state".
"""

import fcntl
import hashlib
import json
import os
import shutil
import tempfile

RUNNER_REQUEST_FORMAT_VERSION = "llm-query-v1"

_JSON_FILES = {"meta.json", "raw_response.json", "validation.json", "usage.json"}


class CacheCorrupt(Exception):
    def __init__(self, key, reason):
        super().__init__(f"cache entry {key} is corrupt: {reason}")
        self.key = key
        self.reason = reason


def compute_cache_key(manifest, provider, model, params,
                      runner_format_version=RUNNER_REQUEST_FORMAT_VERSION):
    """`params` is the caller's dict of response-affecting request settings
    (sampling temperature, reasoning effort, max output tokens, ...) --
    whatever the provider adapter considers part of its request shape.
    Anything NOT in this fixed field list (a timestamp, a request ID, jobs
    concurrency, --refresh) must never influence the key."""
    fields = {
        "prompt_hash": manifest["prompt_hash"],
        "input_hash": manifest["input_hash"],
        "prompt_version": manifest["prompt_version"],
        "response_schema_version": manifest["response_schema_version"],
        "provider": provider,
        "model": model,
        "params": params,
        "runner_request_format_version": runner_format_version,
    }
    canonical = json.dumps(fields, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


class Cache:
    def __init__(self, cache_dir):
        self.cache_dir = cache_dir
        os.makedirs(cache_dir, exist_ok=True)

    def _entry_dir(self, key):
        return os.path.join(self.cache_dir, key[:2], key)

    def lock(self, key):
        """Blocks until an exclusive lock on `key` is acquired; returns an
        open file object the caller must close() to release it."""
        shard = os.path.join(self.cache_dir, key[:2])
        os.makedirs(shard, exist_ok=True)
        f = open(os.path.join(shard, key + ".lock"), "a+")
        fcntl.flock(f.fileno(), fcntl.LOCK_EX)
        return f

    def read(self, key):
        """Returns {filename: parsed_content} for a complete entry, or None
        if no entry exists for `key`. Raises CacheCorrupt if an entry
        directory exists but any file is missing or its hash disagrees with
        integrity.json -- a corrupt entry is deliberately never returned as
        if it were valid or absent."""
        d = self._entry_dir(key)
        integrity_path = os.path.join(d, "integrity.json")
        if not os.path.isfile(integrity_path):
            return None
        with open(integrity_path) as f:
            integrity = json.load(f)
        entry = {}
        for name, expected_hash in integrity.items():
            path = os.path.join(d, name)
            if not os.path.isfile(path):
                raise CacheCorrupt(key, f"missing file {name}")
            with open(path, "rb") as f2:
                data = f2.read()
            if hashlib.sha256(data).hexdigest() != expected_hash:
                raise CacheCorrupt(key, f"hash mismatch for {name}")
            text = data.decode("utf-8")
            entry[name] = json.loads(text) if name in _JSON_FILES else text
        return entry

    def write(self, key, meta, raw_response, extracted_text, validation, usage):
        """Atomically writes a complete entry: meta/raw_response/validation/
        usage are JSON-serializable dicts; extracted_text is the provider's
        extracted response text as a plain string (may or may not itself be
        valid JSON -- that is exactly what `validation`'s state records)."""
        d = self._entry_dir(key)
        shard = os.path.dirname(d)
        os.makedirs(shard, exist_ok=True)
        staging = tempfile.mkdtemp(prefix=f".tmp-{key}-", dir=shard)
        try:
            files = {
                "meta.json": meta,
                "raw_response.json": raw_response,
                "extracted.txt": extracted_text,
                "validation.json": validation,
                "usage.json": usage,
            }
            integrity = {}
            for name, content in files.items():
                data = (
                    json.dumps(content, sort_keys=True, indent=2).encode("utf-8")
                    if name in _JSON_FILES
                    else content.encode("utf-8")
                )
                with open(os.path.join(staging, name), "wb") as f:
                    f.write(data)
                integrity[name] = hashlib.sha256(data).hexdigest()
            with open(os.path.join(staging, "integrity.json"), "w") as f:
                json.dump(integrity, f, sort_keys=True, indent=2)
            if os.path.isdir(d):
                shutil.rmtree(d)
            os.rename(staging, d)
        except BaseException:
            shutil.rmtree(staging, ignore_errors=True)
            raise
