#!/usr/bin/env bash
# Black-box regression suite for llm_query.py, driven entirely through
# --provider fake (see provider.py's FakeProvider) -- no network access, no
# real token, ever required. Covers plan section 9's checklist: cache
# miss/hit, changed prompt/model/schema keys, atomic resume, concurrent
# duplicate suppression, transient retry, permanent failure, token
# aggregation, absent usage fields. Pure-Python unit tests (validator
# fixtures, OpenAIProvider HTTP classification/secret redaction) live in
# test_validator.py/test_provider_openai.py and are run first.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TOOL_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
LLM_QUERY="$TOOL_DIR/llm_query.py"
MAKE_FIXTURE="$SCRIPT_DIR/make_fixture.py"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

PASS=0
FAIL=0

check() {
    local desc="$1" got="$2" want="$3"
    if [[ "$got" == "$want" ]]; then
        echo "  ok    $desc"
        PASS=$((PASS + 1))
    else
        echo "  FAIL  $desc"
        echo "        got:  $got"
        echo "        want: $want"
        FAIL=$((FAIL + 1))
    fi
}

pyjq() {
    python3 -c "
import json, sys
f = json.load(open(sys.argv[1]))
print($2)
" "$1"
}

single_ptr_args='[{"id":"arg0","name":"n","llvm_type":"i32"},{"id":"arg1","name":"x","llvm_type":"ptr"}]'

usable_response() {
    local fn="$1"
    python3 -c "
import json
print(json.dumps({'response_schema_version':'marshal-response-v6','function':'$fn',
  'pointer_arguments':[{'id':'arg1','direction':'in','extent':'one'}]}))
"
}

fake_script() {
    # fake_script <out_path> <python dict literal building the script>
    python3 -c "
import json
print(json.dumps($2))
" > "$1"
}

echo "=== 1. cache miss then hit ==="
mkdir -p "$WORK/prompts1"
python3 "$MAKE_FIXTURE" "$WORK/prompts1" fn1 "$single_ptr_args"
resp1="$(usable_response fn1)"
fake_script "$WORK/script1.json" "{'fn1': [{'kind':'response','response':$resp1,'usage':{'input_tokens':10,'output_tokens':5,'total_tokens':15}}]}"

python3 "$LLM_QUERY" --prompt-dir "$WORK/prompts1" --cache-dir "$WORK/cache1" \
    --provider fake --fake-script "$WORK/script1.json" --model test-model \
    --summary "$WORK/summary1a.json" --jobs 1 >"$WORK/run1a.log" 2>&1
check "1: exit code" "$?" "0"
check "1: first run is a cache miss (1 new call)" "$(pyjq "$WORK/summary1a.json" "f['new_api_calls']")" "1"
check "1: first run has zero cache hits" "$(pyjq "$WORK/summary1a.json" "f['cache_hits']")" "0"
check "1: first run classified usable" "$(pyjq "$WORK/summary1a.json" "f['counts_by_state'].get('usable')")" "1"

python3 "$LLM_QUERY" --prompt-dir "$WORK/prompts1" --cache-dir "$WORK/cache1" \
    --provider fake --fake-script "$WORK/script1.json" --model test-model \
    --summary "$WORK/summary1b.json" --jobs 1 >"$WORK/run1b.log" 2>&1
check "1: second identical run makes zero new API calls" "$(pyjq "$WORK/summary1b.json" "f['new_api_calls']")" "0"
check "1: second identical run is a cache hit" "$(pyjq "$WORK/summary1b.json" "f['cache_hits']")" "1"

echo ""
echo "=== 2. changed model produces a different cache key (new call) ==="
python3 "$LLM_QUERY" --prompt-dir "$WORK/prompts1" --cache-dir "$WORK/cache1" \
    --provider fake --fake-script "$WORK/script1.json" --model test-model-v2 \
    --summary "$WORK/summary2.json" --jobs 1 >"$WORK/run2.log" 2>&1
check "2: exit code" "$?" "0"
check "2: different model is NOT a cache hit" "$(pyjq "$WORK/summary2.json" "f['new_api_calls']")" "1"

echo ""
echo "=== 3. changed response-affecting param produces a different cache key ==="
python3 "$LLM_QUERY" --prompt-dir "$WORK/prompts1" --cache-dir "$WORK/cache1" \
    --provider fake --fake-script "$WORK/script1.json" --model test-model \
    --param 'temperature=0' --summary "$WORK/summary3.json" --jobs 1 >"$WORK/run3.log" 2>&1
check "3: exit code" "$?" "0"
check "3: added param is NOT a cache hit" "$(pyjq "$WORK/summary3.json" "f['new_api_calls']")" "1"

echo ""
echo "=== 4. resume: a permanent failure is retried automatically; a success is not ==="
mkdir -p "$WORK/prompts4"
python3 "$MAKE_FIXTURE" "$WORK/prompts4" fn_ok "$single_ptr_args"
python3 "$MAKE_FIXTURE" "$WORK/prompts4" fn_fail "$single_ptr_args"
resp_ok="$(usable_response fn_ok)"
resp_fail_retry="$(usable_response fn_fail)"
fake_script "$WORK/script4a.json" "{'fn_ok': [{'kind':'response','response':$resp_ok}], 'fn_fail': [{'kind':'permanent_error'}]}"

python3 "$LLM_QUERY" --prompt-dir "$WORK/prompts4" --cache-dir "$WORK/cache4" \
    --provider fake --fake-script "$WORK/script4a.json" --model test-model \
    --summary "$WORK/summary4a.json" --jobs 1 >"$WORK/run4a.log" 2>&1
check "4a: exit code" "$?" "0"
check "4a: fn_ok usable" "$(pyjq "$WORK/summary4a.json" "[r['state'] for r in f['results'] if r['function']=='fn_ok'][0]")" "usable"
check "4a: fn_fail api_failed" "$(pyjq "$WORK/summary4a.json" "[r['state'] for r in f['results'] if r['function']=='fn_fail'][0]")" "api_failed"
check "4a: two new calls" "$(pyjq "$WORK/summary4a.json" "f['new_api_calls']")" "2"

# Deliberately flip fn_ok to a failure step and fn_fail to a success step:
# if resume semantics are wrong (fn_ok gets re-queried), fn_ok would now
# show api_failed, revealing the bug; if correct, fn_ok is a pure cache hit
# and never touches this script at all.
fake_script "$WORK/script4b.json" "{'fn_ok': [{'kind':'permanent_error'}], 'fn_fail': [{'kind':'response','response':$resp_fail_retry}]}"
python3 "$LLM_QUERY" --prompt-dir "$WORK/prompts4" --cache-dir "$WORK/cache4" \
    --provider fake --fake-script "$WORK/script4b.json" --model test-model \
    --summary "$WORK/summary4b.json" --jobs 1 >"$WORK/run4b.log" 2>&1
check "4b: exit code" "$?" "0"
check "4b: fn_ok is a cache hit, unaffected by script4b" \
    "$(pyjq "$WORK/summary4b.json" "[r['cache_hit'] for r in f['results'] if r['function']=='fn_ok'][0]")" "True"
check "4b: fn_ok still usable" "$(pyjq "$WORK/summary4b.json" "[r['state'] for r in f['results'] if r['function']=='fn_ok'][0]")" "usable"
check "4b: fn_fail was retried (not a cache hit)" \
    "$(pyjq "$WORK/summary4b.json" "[r['cache_hit'] for r in f['results'] if r['function']=='fn_fail'][0]")" "False"
check "4b: fn_fail now usable" "$(pyjq "$WORK/summary4b.json" "[r['state'] for r in f['results'] if r['function']=='fn_fail'][0]")" "usable"
check "4b: exactly one new call (fn_fail only)" "$(pyjq "$WORK/summary4b.json" "f['new_api_calls']")" "1"

echo ""
echo "=== 5. transient-then-success retry ==="
mkdir -p "$WORK/prompts5"
python3 "$MAKE_FIXTURE" "$WORK/prompts5" fn_flaky "$single_ptr_args"
resp_flaky="$(usable_response fn_flaky)"
fake_script "$WORK/script5.json" "{'fn_flaky': [{'kind':'transient_error'}, {'kind':'transient_error'}, {'kind':'response','response':$resp_flaky}]}"
python3 "$LLM_QUERY" --prompt-dir "$WORK/prompts5" --cache-dir "$WORK/cache5" \
    --provider fake --fake-script "$WORK/script5.json" --model test-model \
    --summary "$WORK/summary5.json" --jobs 1 >"$WORK/run5.log" 2>&1
check "5: exit code" "$?" "0"
check "5: eventually usable" "$(pyjq "$WORK/summary5.json" "[r['state'] for r in f['results']][0]")" "usable"
key5="$(pyjq "$WORK/summary5.json" "[r['key'] for r in f['results']][0]")"
check "5: cached usage.json records 3 attempts" \
    "$(python3 -c "import json; print(json.load(open('$WORK/cache5/${key5:0:2}/$key5/usage.json'))['attempts'])")" "3"

echo ""
echo "=== 6. permanent failure never retried within a single run ==="
mkdir -p "$WORK/prompts6"
python3 "$MAKE_FIXTURE" "$WORK/prompts6" fn_dead "$single_ptr_args"
fake_script "$WORK/script6.json" "{'fn_dead': [{'kind':'permanent_error','message':'auth failed'}]}"
python3 "$LLM_QUERY" --prompt-dir "$WORK/prompts6" --cache-dir "$WORK/cache6" \
    --provider fake --fake-script "$WORK/script6.json" --model test-model \
    --max-attempts 5 --summary "$WORK/summary6.json" --jobs 1 >"$WORK/run6.log" 2>&1
check "6: exit code" "$?" "0"
check "6: api_failed" "$(pyjq "$WORK/summary6.json" "f['results'][0]['state']")" "api_failed"
key6="$(pyjq "$WORK/summary6.json" "f['results'][0]['key']")"
check "6: only 1 attempt (permanent error, not retried)" \
    "$(python3 -c "import json; print(json.load(open('$WORK/cache6/${key6:0:2}/$key6/usage.json'))['attempts'])")" "1"

echo ""
echo "=== 7. token aggregation across functions ==="
mkdir -p "$WORK/prompts7"
python3 "$MAKE_FIXTURE" "$WORK/prompts7" fn_a "$single_ptr_args"
python3 "$MAKE_FIXTURE" "$WORK/prompts7" fn_b "$single_ptr_args"
resp_a="$(usable_response fn_a)"; resp_b="$(usable_response fn_b)"
fake_script "$WORK/script7.json" "{'fn_a': [{'kind':'response','response':$resp_a,'usage':{'input_tokens':10,'output_tokens':5,'total_tokens':15}}], 'fn_b': [{'kind':'response','response':$resp_b,'usage':{'input_tokens':7,'output_tokens':3,'total_tokens':10}}]}"
python3 "$LLM_QUERY" --prompt-dir "$WORK/prompts7" --cache-dir "$WORK/cache7" \
    --provider fake --fake-script "$WORK/script7.json" --model test-model \
    --summary "$WORK/summary7.json" --jobs 2 >"$WORK/run7.log" 2>&1
check "7: exit code" "$?" "0"
check "7: aggregated input_tokens" "$(pyjq "$WORK/summary7.json" "f['total_usage_for_new_calls']['input_tokens']")" "17"
check "7: aggregated output_tokens" "$(pyjq "$WORK/summary7.json" "f['total_usage_for_new_calls']['output_tokens']")" "8"
check "7: aggregated total_tokens" "$(pyjq "$WORK/summary7.json" "f['total_usage_for_new_calls']['total_tokens']")" "25"
check "7: cache_effectiveness with no cached tokens at all reports zero, not missing" \
    "$(pyjq "$WORK/summary7.json" "(f['cache_effectiveness']['cached_input_tokens'], f['cache_effectiveness']['requests_with_cached_tokens'])")" "(0, 0)"

echo ""
echo "=== 7b. cache_effectiveness reflects provider-reported cached tokens ==="
mkdir -p "$WORK/prompts7b"
python3 "$MAKE_FIXTURE" "$WORK/prompts7b" fn_c "$single_ptr_args"
python3 "$MAKE_FIXTURE" "$WORK/prompts7b" fn_d "$single_ptr_args"
resp_c="$(usable_response fn_c)"; resp_d="$(usable_response fn_d)"
fake_script "$WORK/script7b.json" "{'fn_c': [{'kind':'response','response':$resp_c,'usage':{'input_tokens':100,'cached_input_tokens':40}}], 'fn_d': [{'kind':'response','response':$resp_d,'usage':{'input_tokens':50,'cached_input_tokens':0}}]}"
python3 "$LLM_QUERY" --prompt-dir "$WORK/prompts7b" --cache-dir "$WORK/cache7b" \
    --provider fake --fake-script "$WORK/script7b.json" --model test-model \
    --summary "$WORK/summary7b.json" --jobs 2 >"$WORK/run7b.log" 2>&1
check "7b: exit code" "$?" "0"
check "7b: cached_input_tokens summed across calls" \
    "$(pyjq "$WORK/summary7b.json" "f['cache_effectiveness']['cached_input_tokens']")" "40"
check "7b: only the call that actually had cached tokens is counted" \
    "$(pyjq "$WORK/summary7b.json" "f['cache_effectiveness']['requests_with_cached_tokens']")" "1"
check "7b: cached_input_token_ratio" \
    "$(pyjq "$WORK/summary7b.json" "f['cache_effectiveness']['cached_input_token_ratio']")" "0.26666666666666666"

echo ""
echo "=== 8. absent usage fields are omitted, not invented as zero ==="
mkdir -p "$WORK/prompts8"
python3 "$MAKE_FIXTURE" "$WORK/prompts8" fn_nousage "$single_ptr_args"
resp_nousage="$(usable_response fn_nousage)"
fake_script "$WORK/script8.json" "{'fn_nousage': [{'kind':'response','response':$resp_nousage}]}"
python3 "$LLM_QUERY" --prompt-dir "$WORK/prompts8" --cache-dir "$WORK/cache8" \
    --provider fake --fake-script "$WORK/script8.json" --model test-model \
    --summary "$WORK/summary8.json" --jobs 1 >"$WORK/run8.log" 2>&1
check "8: exit code" "$?" "0"
key8="$(pyjq "$WORK/summary8.json" "f['results'][0]['key']")"
check "8: cached tokens dict is empty, not zero-filled" \
    "$(python3 -c "import json; print(json.load(open('$WORK/cache8/${key8:0:2}/$key8/usage.json'))['tokens'])")" "{}"

echo ""
echo "=== 9. concurrent duplicate suppression: two manifests sharing one cache key ==="
mkdir -p "$WORK/prompts9"
shared_text=$'=== TASK ===\nShared prompt text for the concurrency test.\n'
python3 "$MAKE_FIXTURE" "$WORK/prompts9" dup_a "$single_ptr_args" '{}' "$shared_text"
python3 "$MAKE_FIXTURE" "$WORK/prompts9" dup_b "$single_ptr_args" '{"input_hash": "sha256:'"$(python3 -c "import hashlib; print(hashlib.sha256(b'dup_a').hexdigest())")"'"}' "$shared_text"
# dup_a's own input_hash is sha256("dup_a") by make_fixture's default; force
# dup_b's to match it too, so both manifests agree on prompt_hash (same
# text) AND input_hash (forced equal) -> the identical cache key.
resp_dup_a="$(usable_response dup_a)"
resp_dup_b="$(usable_response dup_b)"
fake_script "$WORK/script9.json" "{'dup_a': [{'kind':'response','response':$resp_dup_a,'delay_s':0.3}], 'dup_b': [{'kind':'response','response':$resp_dup_b,'delay_s':0.3}]}"
python3 "$LLM_QUERY" --prompt-dir "$WORK/prompts9" --cache-dir "$WORK/cache9" \
    --provider fake --fake-script "$WORK/script9.json" --model test-model \
    --summary "$WORK/summary9.json" --jobs 2 >"$WORK/run9.log" 2>&1
check "9: exit code" "$?" "0"
check "9: both manifests computed the same cache key" \
    "$(pyjq "$WORK/summary9.json" "len(set(r['key'] for r in f['results'])) == 1")" "True"
check "9: exactly one of the pair made a new API call (the lock suppressed the duplicate)" \
    "$(pyjq "$WORK/summary9.json" "f['new_api_calls']")" "1"
check "9: exactly one of the pair is reported as a cache hit" \
    "$(pyjq "$WORK/summary9.json" "f['cache_hits']")" "1"
check "9: both ended up usable (the one that hit reused the winner's answer)" \
    "$(pyjq "$WORK/summary9.json" "all(r['state']=='usable' for r in f['results'])")" "True"

echo ""
echo "=== 10. local ineligibility: slice_complete=false and hash mismatch are never queried ==="
mkdir -p "$WORK/prompts10"
python3 "$MAKE_FIXTURE" "$WORK/prompts10" fn_incomplete "$single_ptr_args" '{"slice_complete": false, "eligible_for_llm_inference": false}'
python3 "$MAKE_FIXTURE" "$WORK/prompts10" fn_badhash "$single_ptr_args" '{"prompt_hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000"}'
fake_script "$WORK/script10.json" "{}"
python3 "$LLM_QUERY" --prompt-dir "$WORK/prompts10" --cache-dir "$WORK/cache10" \
    --provider fake --fake-script "$WORK/script10.json" --model test-model \
    --summary "$WORK/summary10.json" --jobs 1 >"$WORK/run10.log" 2>&1
check "10: exit code" "$?" "0"
check "10: zero queryable functions" "$(pyjq "$WORK/summary10.json" "f['queryable']")" "0"
check "10: ineligible_manifest rejection counted" \
    "$(pyjq "$WORK/summary10.json" "f['locally_rejected'].get('ineligible_manifest')")" "1"
check "10: hash_mismatch rejection counted" \
    "$(pyjq "$WORK/summary10.json" "f['locally_rejected'].get('hash_mismatch')")" "1"

echo ""
echo "=== 11. malformed/unknown model answers are cached as completed, non-usable attempts ==="
mkdir -p "$WORK/prompts11"
python3 "$MAKE_FIXTURE" "$WORK/prompts11" fn_badjson "$single_ptr_args"
python3 "$MAKE_FIXTURE" "$WORK/prompts11" fn_unknown "$single_ptr_args"
fake_script "$WORK/script11.json" "{'fn_badjson': [{'kind':'response','text':'Here you go: {not valid json'}], 'fn_unknown': [{'kind':'response','response':{'response_schema_version':'marshal-response-v6','function':'fn_unknown','pointer_arguments':[{'id':'arg1','direction':'unknown','extent':'unknown'}]}}]}"
python3 "$LLM_QUERY" --prompt-dir "$WORK/prompts11" --cache-dir "$WORK/cache11" \
    --provider fake --fake-script "$WORK/script11.json" --model test-model \
    --summary "$WORK/summary11.json" --jobs 1 >"$WORK/run11.log" 2>&1
check "11: exit code" "$?" "0"
check "11: malformed JSON -> json_invalid" \
    "$(pyjq "$WORK/summary11.json" "[r['state'] for r in f['results'] if r['function']=='fn_badjson'][0]")" "json_invalid"
check "11: unknown classification -> model_unknown" \
    "$(pyjq "$WORK/summary11.json" "[r['state'] for r in f['results'] if r['function']=='fn_unknown'][0]")" "model_unknown"
check "11: both counted as new API calls (billed attempts)" "$(pyjq "$WORK/summary11.json" "f['new_api_calls']")" "2"

# Rerun WITHOUT --retry-invalid: neither invalid/unknown attempt is re-queried.
python3 "$LLM_QUERY" --prompt-dir "$WORK/prompts11" --cache-dir "$WORK/cache11" \
    --provider fake --fake-script "$WORK/script11.json" --model test-model \
    --summary "$WORK/summary11b.json" --jobs 1 >"$WORK/run11b.log" 2>&1
check "11b: normal rerun makes zero new calls for invalid/unknown attempts" \
    "$(pyjq "$WORK/summary11b.json" "f['new_api_calls']")" "0"

echo ""
echo "=== 12. --retry-invalid re-queries invalid/unknown but not usable entries ==="
resp_unknown_fixed="$(python3 -c "
import json
print(json.dumps({'response_schema_version':'marshal-response-v6','function':'fn_unknown',
  'pointer_arguments':[{'id':'arg1','direction':'in','extent':'one'}]}))
")"
resp_badjson_fixed="$(usable_response fn_badjson)"
fake_script "$WORK/script11c.json" "{'fn_badjson': [{'kind':'response','response':$resp_badjson_fixed}], 'fn_unknown': [{'kind':'response','response':$resp_unknown_fixed}]}"
python3 "$LLM_QUERY" --prompt-dir "$WORK/prompts11" --cache-dir "$WORK/cache11" \
    --provider fake --fake-script "$WORK/script11c.json" --model test-model \
    --retry-invalid --summary "$WORK/summary12.json" --jobs 1 >"$WORK/run12.log" 2>&1
check "12: exit code" "$?" "0"
check "12: json_invalid entry IS re-queried under --retry-invalid" \
    "$(pyjq "$WORK/summary12.json" "[r['cache_hit'] for r in f['results'] if r['function']=='fn_badjson'][0]")" "False"
check "12: it is now usable after the retry" \
    "$(pyjq "$WORK/summary12.json" "[r['state'] for r in f['results'] if r['function']=='fn_badjson'][0]")" "usable"
check "12: model_unknown entry IS re-queried under --retry-invalid" \
    "$(pyjq "$WORK/summary12.json" "[r['cache_hit'] for r in f['results'] if r['function']=='fn_unknown'][0]")" "False"
check "12: it is now usable after the retry" \
    "$(pyjq "$WORK/summary12.json" "[r['state'] for r in f['results'] if r['function']=='fn_unknown'][0]")" "usable"

echo ""
echo "=== 13. --refresh re-queries even an already-usable entry ==="
resp1_v2="$(python3 -c "
import json
print(json.dumps({'response_schema_version':'marshal-response-v6','function':'fn1',
  'pointer_arguments':[{'id':'arg1','direction':'out','extent':'one'}]}))
")"
fake_script "$WORK/script13.json" "{'fn1': [{'kind':'response','response':$resp1_v2}]}"
python3 "$LLM_QUERY" --prompt-dir "$WORK/prompts1" --cache-dir "$WORK/cache1" \
    --provider fake --fake-script "$WORK/script13.json" --model test-model \
    --refresh --summary "$WORK/summary13.json" --jobs 1 >"$WORK/run13.log" 2>&1
check "13: exit code" "$?" "0"
check "13: --refresh re-queries an already-usable entry" \
    "$(pyjq "$WORK/summary13.json" "f['results'][0]['cache_hit']")" "False"
check "13: the refreshed answer overwrote the cached one" \
    "$(pyjq "$WORK/summary13.json" "f['new_api_calls']")" "1"

echo ""
echo "=== 13a. --model rejects a model this pipeline hasn't been exercised against ==="
mkdir -p "$WORK/prompts13a"
python3 "$MAKE_FIXTURE" "$WORK/prompts13a" fn1a "$single_ptr_args"
LLM_INFER_API_KEY=unused-in-this-test python3 "$LLM_QUERY" --prompt-dir "$WORK/prompts13a" --cache-dir "$WORK/cache13a" \
    --provider openai --model gpt-4-turbo \
    --summary "$WORK/summary13a.json" --jobs 1 >"$WORK/run13a.log" 2>&1
check "13a: exit code" "$?" "1"
check "13a: error names the unsupported model" \
    "$(grep -c "not a supported OpenAI model" "$WORK/run13a.log")" "1"
check "13a: no summary file was written (nothing was ever queried)" \
    "$([[ -f "$WORK/summary13a.json" ]] && echo yes || echo no)" "no"

echo ""
echo "=== 13b. --param cannot set a reserved OpenAI request field ==="
mkdir -p "$WORK/prompts13b"
python3 "$MAKE_FIXTURE" "$WORK/prompts13b" fn1b "$single_ptr_args"
LLM_INFER_API_KEY=unused-in-this-test python3 "$LLM_QUERY" --prompt-dir "$WORK/prompts13b" --cache-dir "$WORK/cache13b" \
    --provider openai --model gpt-5.6-sol --param 'prompt_cache_key="attacker"' \
    --summary "$WORK/summary13b.json" --jobs 1 >"$WORK/run13b.log" 2>&1
check "13b: exit code" "$?" "1"
check "13b: error names the reserved field" \
    "$(grep -c "reserved OpenAI request field" "$WORK/run13b.log")" "1"
check "13b: no summary file was written (nothing was ever queried)" \
    "$([[ -f "$WORK/summary13b.json" ]] && echo yes || echo no)" "no"

echo ""
echo "=== 14. compare_static.py CLI: wires cache lookup + static JSON into a report ==="
COMPARE_STATIC="$TOOL_DIR/compare_static.py"
mkdir -p "$WORK/prompts14"
two_arg='[{"id":"arg0","name":"n","llvm_type":"i32"},{"id":"arg1","name":"x","llvm_type":"ptr"},{"id":"arg2","name":"incx","llvm_type":"i32"}]'
python3 "$MAKE_FIXTURE" "$WORK/prompts14" fn_agree "$two_arg"
python3 "$MAKE_FIXTURE" "$WORK/prompts14" fn_disagree "$two_arg"
python3 "$MAKE_FIXTURE" "$WORK/prompts14" fn_static_only "$two_arg"
resp_agree="$(python3 -c "
import json
print(json.dumps({'response_schema_version':'marshal-response-v6','function':'fn_agree',
  'pointer_arguments':[{'id':'arg1','direction':'in','extent':'stride_vector',
    'extent_operand':{'size':{'source':'argument_value','argument_id':'arg0','constant_value':None},
                       'stride':{'source':'argument_value','argument_id':'arg2','constant_value':None}}}]}))
")"
# fake_script's argument is evaluated as PYTHON SOURCE (not parsed as JSON), so
# a JSON "null" produced by json.dumps above must be spelled "None" here, or
# embedding it verbatim raises NameError when fake_script's own script runs.
resp_agree="${resp_agree//null/None}"
resp_disagree="$(python3 -c "
import json
print(json.dumps({'response_schema_version':'marshal-response-v6','function':'fn_disagree',
  'pointer_arguments':[{'id':'arg1','direction':'out','extent':'stride_vector',
    'extent_operand':{'size':{'source':'argument_value','argument_id':'arg0','constant_value':None},
                       'stride':{'source':'argument_value','argument_id':'arg2','constant_value':None}}}]}))
")"
resp_disagree="${resp_disagree//null/None}"
fake_script "$WORK/script14.json" "{'fn_agree': [{'kind':'response','response':$resp_agree}], 'fn_disagree': [{'kind':'response','response':$resp_disagree}], 'fn_static_only': [{'kind':'permanent_error'}]}"
python3 "$LLM_QUERY" --prompt-dir "$WORK/prompts14" --cache-dir "$WORK/cache14" \
    --provider fake --fake-script "$WORK/script14.json" --model test-model \
    --summary "$WORK/summary14.json" --jobs 1 >"$WORK/run14.log" 2>&1

python3 -c "
import json
static = {
  'module': 'test', 'config': {}, 'function_count': 3,
  'functions': [
    {'name': 'fn_agree', 'decision': 'marshal', 'ret': {'kind':'void'}, 'args': [
        {'kind':'scalar','type':'int','size':4},
        {'kind':'ptr','type':'<ptr>','size':4,'dir':'in','size_kind':'stride_vector',
         'size_operand':{'arg_index':0,'source':'value'},
         'stride_operand':{'arg_index':2,'source':'value'},'const_size':8,
         'confidence':'proven','pointee':[{'kind':'scalar','type':'double','size':8}]},
        {'kind':'scalar','type':'int','size':4},
    ], 'warnings': []},
    {'name': 'fn_disagree', 'decision': 'marshal', 'ret': {'kind':'void'}, 'args': [
        {'kind':'scalar','type':'int','size':4},
        {'kind':'ptr','type':'<ptr>','size':4,'dir':'in','size_kind':'stride_vector',
         'size_operand':{'arg_index':0,'source':'value'},
         'stride_operand':{'arg_index':2,'source':'value'},'const_size':8,
         'confidence':'proven','pointee':[{'kind':'scalar','type':'double','size':8}]},
        {'kind':'scalar','type':'int','size':4},
    ], 'warnings': []},
    {'name': 'fn_static_only', 'decision': 'force_local', 'warnings': ['unresolved']},
  ],
}
json.dump(static, open('$WORK/static14.json', 'w'))
"
python3 "$COMPARE_STATIC" --static-json "$WORK/static14.json" --prompt-dir "$WORK/prompts14" \
    --cache-dir "$WORK/cache14" --provider fake --model test-model \
    --report "$WORK/report14.json" >"$WORK/compare14.log" 2>&1
check "14: exit code" "$?" "0"
check "14: fn_agree's pointer arg is exact_agreement" \
    "$(pyjq "$WORK/report14.json" "[fn for fn in f['functions'] if fn['function']=='fn_agree'][0]['arguments'][0]['category']")" \
    "exact_agreement"
check "14: fn_disagree's pointer arg is direction_disagreement" \
    "$(pyjq "$WORK/report14.json" "[fn for fn in f['functions'] if fn['function']=='fn_disagree'][0]['arguments'][0]['category']")" \
    "direction_disagreement"
check "14: fn_static_only (force_local, LLM api_failed) is neither_available" \
    "$(pyjq "$WORK/report14.json" "[fn for fn in f['functions'] if fn['function']=='fn_static_only'][0]['arguments'][0]['category']")" \
    "neither_available"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[[ "$FAIL" -eq 0 ]]
