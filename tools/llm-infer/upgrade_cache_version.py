#!/usr/bin/env python3
"""Upgrades historical cache entries written under an older
response_schema_version to the current one, so they become real cache
hits for a live llm_query.py run instead of being silently stranded.

This is sound ONLY because every version bump so far has been either (a)
a fix to a self-contradiction in how the OLD validator read the OLD
prompt's own wire format (v4 -> v5: validator expected a bare operand for
constant/argument extents, but the prompt already told the model to send
it wrapped as {"size": <operand>} -- see tools/llm-infer/validator.py's
own history), or (b) a strictly additive vocabulary extension (v5 -> v6
added "max"; "add"/"divide" were added without a version bump at all,
since they're additive too -- see the project's standing rule to only
bump the version for a genuinely incompatible contract change). Neither
kind of change ever altered what an already-produced answer MEANS, only
how it's spelled -- so a deterministic reshape + re-validation recovers
it losslessly. It does NOT and CANNOT upgrade a model_unknown/
schema_invalid answer into a better one: a model that never had "max"
available simply never tried it, and no amount of reshaping invents the
reasoning it would have done with it. Those cases need a fresh query,
not an upgrade.

Two mechanical transforms are applied, both safe no-ops when not needed:
  1. For "constant"/"argument" extents, wrap a bare leaf operand as
     {"size": <operand>} if it isn't already (the v4->v5 fix).
  2. Every leaf operand gets both "argument_id" and "constant_value" keys
     explicitly present (null for the unused one) if either is missing --
     a later, separately-landed strict-mode requirement that predates
     even some "v5"-labeled entries in practice.
After reshaping, the response is re-validated from scratch against
TODAY's validator.py and the function's CURRENT manifest (from
--prompt-dir) -- never trusted blindly.

PROVENANCE IS NEVER REWRITTEN. The upgraded entry's provider/model/params
are always the entry's OWN original values, read from its own meta.json
-- never a caller-supplied override. The new cache key is computed from
those SAME original values plus the current manifest, so the upgraded
entry can only ever become a cache hit for a query that would have used
that exact original provider/model/params anyway. This tool has no way
to make an answer produced by one model masquerade as another's, and
deliberately does not accept a provider/model override for that reason:
an earlier version of this script did take --provider/--model as a
relabeling target, which silently attributed gpt-5.5-produced answers to
gpt-6-sol in the shared sweep cache. If you actually want data FOR a
different model, query that model -- this tool only recovers data that
already exists under its true identity.
"""

import argparse
import copy
import datetime
import glob
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import cache as cache_mod
import validator as validator_mod


def _now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def _normalize_operand(op):
    """Recursively ensures every leaf carries both argument_id and
    constant_value (null for the unused one). Composites are walked but
    not otherwise altered -- their shape never changed across versions."""
    if not isinstance(op, dict):
        return op
    if "op" in op:
        if "operands" in op and isinstance(op["operands"], list):
            op["operands"] = [_normalize_operand(sub) for sub in op["operands"]]
        if "operand" in op:
            op["operand"] = _normalize_operand(op["operand"])
        return op
    op.setdefault("argument_id", None)
    op.setdefault("constant_value", None)
    return op


def upgrade_response_json(obj, target_version):
    """Returns a deep-copied, reshaped version of the parsed response
    object `obj`, with response_schema_version set to `target_version`.
    Does not itself validate the result -- the caller re-validates."""
    obj = copy.deepcopy(obj)
    for entry in obj.get("pointer_arguments") or []:
        extent = entry.get("extent")
        operand = entry.get("extent_operand")
        if not isinstance(operand, dict):
            continue
        if extent in validator_mod.EXTENTS_WITH_SINGLE_OPERAND:
            # v4's bug: a bare leaf/composite directly under extent_operand,
            # instead of wrapped as {"size": <operand>}.
            if "size" not in operand and ("source" in operand or "op" in operand):
                operand = {"size": operand}
                entry["extent_operand"] = operand
            if "size" in operand:
                operand["size"] = _normalize_operand(operand["size"])
        elif extent == "stride_vector":
            if "size" in operand:
                operand["size"] = _normalize_operand(operand["size"])
            if "stride" in operand:
                operand["stride"] = _normalize_operand(operand["stride"])
    obj["response_schema_version"] = target_version
    return obj


def load_current_manifests(prompt_dir):
    manifests = {}
    for path in glob.glob(os.path.join(prompt_dir, "*.prompt.json")):
        function_name = os.path.basename(path)[: -len(".prompt.json")]
        with open(path) as f:
            manifests[function_name] = json.load(f)
    return manifests


def iter_cache_entries(cache_dir):
    """Yields (key, meta, extracted_text) for every complete entry under
    cache_dir -- walks the shard layout directly rather than using
    Cache.read, since we want every entry regardless of key, not a lookup
    by a specific computed key."""
    for integrity_path in glob.glob(os.path.join(cache_dir, "*", "*", "integrity.json")):
        entry_dir = os.path.dirname(integrity_path)
        key = os.path.basename(entry_dir)
        try:
            with open(os.path.join(entry_dir, "meta.json")) as f:
                meta = json.load(f)
            with open(os.path.join(entry_dir, "extracted.txt")) as f:
                extracted_text = f.read()
        except (OSError, json.JSONDecodeError):
            continue
        yield key, meta, extracted_text


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--source-cache-dir", action="append", required=True,
                    help="a historical cache directory to scan (repeatable)")
    ap.add_argument("--target-cache-dir", required=True,
                    help="cache directory to write upgraded, reusable entries into")
    ap.add_argument("--prompt-dir", required=True,
                    help="CURRENT --llm-prompt-only output directory (for manifests + cache keys)")
    args = ap.parse_args()

    manifests = load_current_manifests(args.prompt_dir)
    target_cache = cache_mod.Cache(args.target_cache_dir)
    target_version = validator_mod.RESPONSE_SCHEMA_VERSION

    counts = {"scanned": 0, "already_current": 0, "not_in_current_manifests": 0,
             "missing_identity": 0, "json_invalid": 0, "already_covered": 0,
             "upgraded": 0, "still_invalid": 0}

    for source_dir in args.source_cache_dir:
        for key, meta, extracted_text in iter_cache_entries(source_dir):
            counts["scanned"] += 1
            function_name = meta.get("function")
            source_version = meta.get("response_schema_version")
            original_provider = meta.get("provider")
            original_model = meta.get("model")
            original_params = meta.get("params", {})
            if not original_provider or not original_model:
                counts["missing_identity"] += 1
                continue
            if source_version == target_version:
                counts["already_current"] += 1
                continue
            manifest = manifests.get(function_name)
            if manifest is None:
                counts["not_in_current_manifests"] += 1
                continue
            try:
                obj = json.loads(extracted_text)
            except (json.JSONDecodeError, TypeError, ValueError):
                counts["json_invalid"] += 1
                continue
            if not isinstance(obj, dict):
                counts["json_invalid"] += 1
                continue

            upgraded_obj = upgrade_response_json(obj, target_version)
            upgraded_text = json.dumps(upgraded_obj)
            vres = validator_mod.validate_response(upgraded_text, function_name, manifest)
            if vres.state not in (validator_mod.STATE_USABLE, validator_mod.STATE_MODEL_UNKNOWN):
                counts["still_invalid"] += 1
                continue

            # Keyed by the ORIGINAL provider/model/params -- this can only
            # ever satisfy a query that would have used that exact identity.
            new_key = cache_mod.compute_cache_key(manifest, original_provider, original_model, original_params)
            lock = target_cache.lock(new_key)
            try:
                if target_cache.read(new_key) is not None:
                    counts["already_covered"] += 1
                    continue
                new_meta = {
                    "function": function_name,
                    "prompt_hash": manifest["prompt_hash"],
                    "input_hash": manifest["input_hash"],
                    "prompt_version": manifest["prompt_version"],
                    "response_schema_version": target_version,
                    "provider": original_provider,
                    "model": original_model,
                    "params": original_params,
                    "runner_request_format_version": cache_mod.RUNNER_REQUEST_FORMAT_VERSION,
                    "requested_at": meta.get("requested_at"),
                    "original_response_schema_version": source_version,
                    "normalized_response_schema_version": target_version,
                    "provenance": "migrated",
                    "migrated_from_cache_dir": source_dir,
                    "migrated_from_key": key,
                    "migrated_at": _now(),
                }
                usage = {"tokens": {}, "latency_s": None, "attempts": None,
                         "request_id": None, "completed_at": meta.get("requested_at")}
                target_cache.write(new_key, new_meta, {"migrated": True, "original_key": key},
                                   upgraded_text, vres.to_dict(), usage)
                counts["upgraded"] += 1
            finally:
                lock.close()

    print(f"upgrade_cache_version: {json.dumps(counts)}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
