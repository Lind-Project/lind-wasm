#!/usr/bin/env python3
"""Writes a minimal, self-consistent *.prompt.{txt,json} fixture pair for
llm-infer's own test suite. NOT a substitute for marshal-infer's real prompt
generation -- just enough shape (entry_arguments, a prompt_hash that
genuinely matches the .txt bytes) for the runner/validator/cache to
exercise against, without depending on the LLVM-based toolchain being built.

Usage: make_fixture.py <out_dir> <function_name> <entry_arguments_json> [overrides_json] [prompt_text_override]

`prompt_text_override`, if non-empty, is used VERBATIM as the prompt's exact
bytes instead of the function-name-derived template -- used to give two
DIFFERENT function names byte-identical prompt text (needed to force two
manifests onto the same cache key for concurrency tests).
"""
import hashlib
import json
import os
import sys


def main():
    out_dir, function_name, entry_arguments_json = sys.argv[1], sys.argv[2], sys.argv[3]
    overrides = json.loads(sys.argv[4]) if len(sys.argv) > 4 and sys.argv[4] else {}
    prompt_text_override = sys.argv[5] if len(sys.argv) > 5 else ""

    entry_arguments = json.loads(entry_arguments_json)
    prompt_text = prompt_text_override or f"=== TASK ===\nInfer the marshalling semantics of {function_name}.\n"
    prompt_hash = "sha256:" + hashlib.sha256(prompt_text.encode("utf-8")).hexdigest()

    manifest = {
        "format_version": 1,
        "prompt_version": "marshal-ir-v2",
        "response_schema_version": "marshal-response-v6",
        "function": function_name,
        "entry_arguments": entry_arguments,
        "included_functions": [function_name],
        "call_edges": [],
        "slice_complete": True,
        "eligible_for_llm_inference": True,
        "notes": [],
        "limits": {"call_depth": 2, "function_count": 8, "max_instructions": 6000},
        "included_instruction_count": 5,
        "input_hash": "sha256:" + hashlib.sha256(function_name.encode("utf-8")).hexdigest(),
        "prompt_hash": prompt_hash,
    }
    manifest.update(overrides)

    os.makedirs(out_dir, exist_ok=True)
    with open(os.path.join(out_dir, function_name + ".prompt.txt"), "w") as f:
        f.write(prompt_text)
    with open(os.path.join(out_dir, function_name + ".prompt.json"), "w") as f:
        json.dump(manifest, f)


if __name__ == "__main__":
    main()
