#!/usr/bin/env bash
# Regression suite for the offline evidence-collection and prompt-preview
# pipeline (marshal-infer --llm-prompt-only). No network request, no model
# SDK, no marshal decision is made anywhere in this suite or the code it
# tests.
#
# Fixtures are compiled via the real toolchain entry point
# (`lind_compile --emit-llvm` -- see CLAUDE.md) wherever a C identifier can
# express the shape under test; the one exception (unusual_symbol.ll) is
# documented at its own use site below. C fixtures shared with the
# stride_vector_extent suite are referenced there rather than duplicated.
#
# Usage: tools/marshal-infer/tests/llm_prompt/run.sh
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
SVE_DIR="$(cd "$SCRIPT_DIR/../stride_vector_extent" && pwd)"
LIND_COMPILE="$REPO_ROOT/scripts/lind_compile"
MARSHAL_INFER="$REPO_ROOT/tools/marshal-infer/build/marshal-infer"

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

# strict_json_ok <file>: parses strictly (no duplicate object keys silently
# shadowing each other -- json.load's default keeps only the LAST value for
# a repeated key, which would hide a real generator bug) and prints "ok" or
# a specific failure reason.
strict_json_ok() {
    python3 -c "
import json, sys

def no_dup_hook(pairs):
    seen = set()
    for k, _ in pairs:
        if k in seen:
            raise ValueError(f'duplicate key: {k!r}')
        seen.add(k)
    return dict(pairs)

try:
    json.load(open(sys.argv[1]), object_pairs_hook=no_dup_hook)
    print('ok')
except Exception as e:
    print(f'FAIL: {e}')
" "$1"
}

# compile_llm <name> [extra clang args...]: copies $SCRIPT_DIR/<name>.c
# into $WORK and compiles it there via the real toolchain entry point
# (lind_compile writes its output NEXT TO the source it's given -- copying
# first keeps the checked-in fixture directory free of generated
# .bc/.wasm/.cwasm artifacts, the same convention every other fixture in
# this suite already follows).
compile_llm() {
    local name="$1"; shift
    cp "$SCRIPT_DIR/$name.c" "$WORK/$name.c"
    ( cd "$WORK" && "$LIND_COMPILE" --emit-llvm "$name.c" -- "$@" ) \
        > "$WORK/$name.compile.log" 2>&1
}

# run_llm <out_prefix> <function> <bc files...> -- -- <extra marshal-infer flags...>
# Runs marshal-infer --llm-prompt-only, writing into a fresh subdirectory of
# $WORK named after out_prefix; prints that subdirectory's path on success.
run_llm() {
    local out_prefix="$1" fn="$2"; shift 2
    local bcs=()
    while [[ "$#" -gt 0 && "$1" != "--" ]]; do bcs+=("$1"); shift; done
    shift # consume "--"
    local extra=()
    while [[ "$#" -gt 0 ]]; do extra+=("$1"); shift; done

    local outdir="$WORK/out_$out_prefix"
    mkdir -p "$outdir"
    "$MARSHAL_INFER" "${bcs[@]}" --llm-prompt-only --function "$fn" \
        --prompt-output "$outdir" "${extra[@]}" \
        > "$WORK/$out_prefix.run.log" 2>&1
    echo "$?" > "$WORK/$out_prefix.exitcode"
    echo "$outdir"
}

sanitized_base() {
    python3 -c "
import re, sys
name = sys.argv[1]
out = re.sub(r'[^A-Za-z0-9_.-]', '_', name)
if not out:
    out = 'function'
if out in ('.', '..') or out.startswith('.'):
    out = 'fn_' + out
print(out)
" "$1"
}

echo "=== 1. direct pointer access, no callees ==="
cp "$SVE_DIR/scalar_out.c" "$WORK/direct1.c"
if ! ( cd "$WORK" && "$LIND_COMPILE" --emit-llvm direct1.c ) >"$WORK/direct1.log" 2>&1; then
    echo "  FAIL  1: compile failed"; cat "$WORK/direct1.log"; FAIL=$((FAIL+1))
else
    out="$(run_llm direct halve_and_report "$WORK/direct1.bc" --)"
    base="$(sanitized_base halve_and_report)"
    check "1: exit code" "$(cat "$WORK/direct.exitcode")" "0"
    check "1: no callees" "$(pyjq "$out/$base.prompt.json" "f['included_functions']")" "['halve_and_report']"
    check "1: slice_complete" "$(pyjq "$out/$base.prompt.json" "f['slice_complete']")" "True"
    check "1: eligible_for_llm_inference" "$(pyjq "$out/$base.prompt.json" "f['eligible_for_llm_inference']")" "True"
    check "1: strict JSON, no dup keys" "$(strict_json_ok "$out/$base.prompt.json")" "ok"
fi

echo ""
echo "=== 2. wrapper delegating to a worker in another module ==="
cp "$SVE_DIR/wrapper.c" "$WORK/wrapper.c"
cp "$SVE_DIR/worker.c" "$WORK/worker.c"
( cd "$WORK" && "$LIND_COMPILE" --emit-llvm wrapper.c && "$LIND_COMPILE" --emit-llvm worker.c ) >"$WORK/wrapper2.log" 2>&1
out="$(run_llm wrapper wrapper_axpy "$WORK/wrapper.bc" "$WORK/worker.bc" --)"
base="$(sanitized_base wrapper_axpy)"
check "2: exit code" "$(cat "$WORK/wrapper.exitcode")" "0"
check "2: includes both functions" \
    "$(pyjq "$out/$base.prompt.json" "sorted(f['included_functions'])")" \
    "['worker_axpy', 'wrapper_axpy']"
check "2: slice_complete" "$(pyjq "$out/$base.prompt.json" "f['slice_complete']")" "True"
check "2: full 6-argument correspondence" \
    "$(pyjq "$out/$base.prompt.json" "len(f['call_edges'][0]['argument_map'])")" "6"
check "2: repeated generation is byte-identical (prompt)" \
    "$(diff -q "$out/$base.prompt.txt" "$(run_llm wrapper_repeat wrapper_axpy "$WORK/wrapper.bc" "$WORK/worker.bc" --)/$base.prompt.txt" >/dev/null 2>&1 && echo same)" \
    "same"
hash1="$(pyjq "$out/$base.prompt.json" "f['prompt_hash']")"
hash2="$(pyjq "$WORK/out_wrapper_repeat/$base.prompt.json" "f['prompt_hash']")"
check "14: deterministic prompt_hash across repeated runs" "$hash1" "$hash2"
inhash1="$(pyjq "$out/$base.prompt.json" "f['input_hash']")"
inhash2="$(pyjq "$WORK/out_wrapper_repeat/$base.prompt.json" "f['input_hash']")"
check "14: deterministic input_hash across repeated runs" "$inhash1" "$inhash2"

echo ""
echo "=== 3. one caller argument passed to two callee parameters ==="
if ! compile_llm dup_arg; then
    echo "  FAIL  3: compile failed"; cat "$WORK/dup_arg.compile.log"; FAIL=$((FAIL+1))
else
    out="$(run_llm dup caller_dup "$WORK/dup_arg.bc" --)"
    base="$(sanitized_base caller_dup)"
    check "3: exit code" "$(cat "$WORK/dup.exitcode")" "0"
    check "3: same caller arg under both callee params" \
        "$(pyjq "$out/$base.prompt.json" "sorted(e['callee_argument'] for e in f['call_edges'][0]['argument_map'] if e['caller_arguments']==['arg0'])")" \
        "['arg0', 'arg1']"
fi

echo ""
echo "=== 4. multiple caller arguments merged (select) into one callee parameter ==="
if ! compile_llm merge_arg; then
    echo "  FAIL  4: compile failed"; cat "$WORK/merge_arg.compile.log"; FAIL=$((FAIL+1))
else
    out="$(run_llm merge caller_merge "$WORK/merge_arg.bc" --)"
    base="$(sanitized_base caller_merge)"
    check "4: exit code" "$(cat "$WORK/merge.exitcode")" "0"
    check "4: both caller args merged into one callee arg" \
        "$(pyjq "$out/$base.prompt.json" "sorted(f['call_edges'][0]['argument_map'][0]['caller_arguments'])")" \
        "['arg1', 'arg2']"
fi

echo ""
echo "=== 5. missing external callee body ==="
cp "$SVE_DIR/missing_callee.c" "$WORK/missing_callee.c"
( cd "$WORK" && "$LIND_COMPILE" --emit-llvm missing_callee.c ) >"$WORK/missing.log" 2>&1
out="$(run_llm missing wrapper_missing "$WORK/missing_callee.bc" --)"
base="$(sanitized_base wrapper_missing)"
check "5: exit code" "$(cat "$WORK/missing.exitcode")" "0"
check "5: slice_complete false" "$(pyjq "$out/$base.prompt.json" "f['slice_complete']")" "False"
check "5: eligible_for_llm_inference false" "$(pyjq "$out/$base.prompt.json" "f['eligible_for_llm_inference']")" "False"
check "5: an incomplete note names the missing callee" \
    "$(pyjq "$out/$base.prompt.json" "any(n['kind']=='incomplete' and 'undefined_worker' in n['message'] and 'no available body' in n['message'] for n in f['notes'])")" \
    "True"

echo ""
echo "=== 6. pointer passed to an indirect callee ==="
cp "$SVE_DIR/indirect_callee.c" "$WORK/indirect_callee.c"
( cd "$WORK" && "$LIND_COMPILE" --emit-llvm indirect_callee.c ) >"$WORK/indirect6.log" 2>&1
out="$(run_llm indirect6 wrapper_indirect "$WORK/indirect_callee.bc" --)"
base="$(sanitized_base wrapper_indirect)"
check "6: exit code" "$(cat "$WORK/indirect6.exitcode")" "0"
check "6: slice_complete false" "$(pyjq "$out/$base.prompt.json" "f['slice_complete']")" "False"
check "6: an incomplete note describes an indirect call reached by a traced pointer" \
    "$(pyjq "$out/$base.prompt.json" "any(n['kind']=='incomplete' and 'indirect call' in n['message'] and 'passed to' in n['message'] for n in f['notes'])")" \
    "True"

echo ""
echo "=== 7. pointer used as the indirect call target itself ==="
if ! compile_llm indirect_target; then
    echo "  FAIL  7: compile failed"; cat "$WORK/indirect_target.compile.log"; FAIL=$((FAIL+1))
else
    out="$(run_llm indirect7 caller_ptr_as_target "$WORK/indirect_target.bc" --)"
    base="$(sanitized_base caller_ptr_as_target)"
    check "7: exit code" "$(cat "$WORK/indirect7.exitcode")" "0"
    check "7: slice_complete false" "$(pyjq "$out/$base.prompt.json" "f['slice_complete']")" "False"
    check "7: an incomplete note describes the pointer itself as the call target" \
        "$(pyjq "$out/$base.prompt.json" "any(n['kind']=='incomplete' and 'indirect function target' in n['message'] for n in f['notes'])")" \
        "True"
fi

echo ""
echo "=== 8. pointer stored into memory ==="
if ! compile_llm store_escape; then
    echo "  FAIL  8: compile failed"; cat "$WORK/store_escape.compile.log"; FAIL=$((FAIL+1))
else
    out="$(run_llm store caller_store "$WORK/store_escape.bc" --)"
    base="$(sanitized_base caller_store)"
    check "8: exit code" "$(cat "$WORK/store.exitcode")" "0"
    check "8: slice_complete false" "$(pyjq "$out/$base.prompt.json" "f['slice_complete']")" "False"
    check "8: an incomplete note describes the store" \
        "$(pyjq "$out/$base.prompt.json" "any(n['kind']=='incomplete' and 'stored to memory' in n['message'] for n in f['notes'])")" \
        "True"
fi

echo ""
echo "=== 9. memcpy/memmove/memset represented as complete visible evidence ==="
if ! compile_llm memcpy_evidence; then
    echo "  FAIL  9: compile failed"; cat "$WORK/memcpy_evidence.compile.log"; FAIL=$((FAIL+1))
else
    out="$(run_llm memcpy caller_memcpy "$WORK/memcpy_evidence.bc" --)"
    base="$(sanitized_base caller_memcpy)"
    check "9: exit code" "$(cat "$WORK/memcpy.exitcode")" "0"
    check "9: slice_complete STILL true" "$(pyjq "$out/$base.prompt.json" "f['slice_complete']")" "True"
    check "9: an informational (not incomplete) note describes the memcpy" \
        "$(pyjq "$out/$base.prompt.json" "any(n['kind']=='informational' and 'memcpy' in n['message'] for n in f['notes'])")" \
        "True"
    check "9: no incomplete note at all" \
        "$(pyjq "$out/$base.prompt.json" "any(n['kind']=='incomplete' for n in f['notes'])")" \
        "False"
fi

echo ""
echo "=== 10. call-depth truncation ==="
out="$(run_llm depth0 wrapper_axpy "$WORK/wrapper.bc" "$WORK/worker.bc" -- --llm-max-call-depth 0)"
base="$(sanitized_base wrapper_axpy)"
check "10: exit code" "$(cat "$WORK/depth0.exitcode")" "0"
check "10: entry still included" "$(pyjq "$out/$base.prompt.json" "f['included_functions']")" "['wrapper_axpy']"
check "10: slice_complete false" "$(pyjq "$out/$base.prompt.json" "f['slice_complete']")" "False"
check "10: an incomplete note cites the call-depth limit" \
    "$(pyjq "$out/$base.prompt.json" "any(n['kind']=='incomplete' and 'call-depth limit' in n['message'] for n in f['notes'])")" \
    "True"

echo ""
echo "=== 11. function-count truncation ==="
if ! compile_llm two_callees; then
    echo "  FAIL  11: compile failed"; cat "$WORK/two_callees.compile.log"; FAIL=$((FAIL+1))
else
    out="$(run_llm twofn caller_two_callees "$WORK/two_callees.bc" -- --llm-max-functions 2)"
    base="$(sanitized_base caller_two_callees)"
    check "11: exit code" "$(cat "$WORK/twofn.exitcode")" "0"
    check "11: exactly 2 functions included" \
        "$(pyjq "$out/$base.prompt.json" "len(f['included_functions'])")" "2"
    check "11: slice_complete false" "$(pyjq "$out/$base.prompt.json" "f['slice_complete']")" "False"
    check "11: an incomplete note cites the function-count limit" \
        "$(pyjq "$out/$base.prompt.json" "any(n['kind']=='incomplete' and 'function slice limit' in n['message'] for n in f['notes'])")" \
        "True"
fi

echo ""
echo "=== 12. instruction-budget truncation, including an oversized entry ==="
if ! compile_llm large_entry; then
    echo "  FAIL  12: compile failed"; cat "$WORK/large_entry.compile.log"; FAIL=$((FAIL+1))
else
    # A budget far smaller than any reasonable compiled body of
    # large_entry.c's 40+ arithmetic statements -- the exact real
    # instruction count doesn't matter, only that it's provably larger
    # than this.
    out="$(run_llm largebudget large_entry "$WORK/large_entry.bc" -- --llm-max-instructions 5)"
    base="$(sanitized_base large_entry)"
    check "12: exit code" "$(cat "$WORK/largebudget.exitcode")" "0"
    check "12: entry still included despite exceeding budget" \
        "$(pyjq "$out/$base.prompt.json" "f['included_functions']")" "['large_entry']"
    check "12: slice_complete false" "$(pyjq "$out/$base.prompt.json" "f['slice_complete']")" "False"
    check "12: reported instruction count exceeds the configured limit" \
        "$(pyjq "$out/$base.prompt.json" "f['included_instruction_count'] > f['limits']['max_instructions']")" \
        "True"
    check "12: an incomplete note names the entry itself as the cause" \
        "$(pyjq "$out/$base.prompt.json" "any(n['kind']=='incomplete' and 'entry function' in n['message'] and 'exceeds' in n['message'] for n in f['notes'])")" \
        "True"
fi

echo ""
echo "=== recursion (direct self-recursion) ==="
if ! compile_llm recursive; then
    echo "  FAIL  recursion: compile failed"; cat "$WORK/recursive.compile.log"; FAIL=$((FAIL+1))
else
    out="$(run_llm recur recursive_walk "$WORK/recursive.bc" --)"
    base="$(sanitized_base recursive_walk)"
    check "recursion: exit code" "$(cat "$WORK/recur.exitcode")" "0"
    check "recursion: slice_complete false" "$(pyjq "$out/$base.prompt.json" "f['slice_complete']")" "False"
    check "recursion: an incomplete note describes a recursive cycle" \
        "$(pyjq "$out/$base.prompt.json" "any(n['kind']=='incomplete' and 'recursive cycle' in n['message'] for n in f['notes'])")" \
        "True"
fi

echo ""
echo "=== 13. a callee without debug information ==="
cp "$SCRIPT_DIR/nodebug_caller.c" "$WORK/nodebug_caller.c"
cp "$SCRIPT_DIR/nodebug_callee.c" "$WORK/nodebug_callee.c"
( cd "$WORK" && "$LIND_COMPILE" --emit-llvm nodebug_caller.c ) >"$WORK/nodebug1.log" 2>&1
# The callee TU is compiled WITHOUT -g specifically -- the point of this
# case is a debug-less CALLEE reached by delegation from a caller that DOES
# have debug info, proving debug info is a label, not a requirement, for
# a function found only via one-hop delegation.
( cd "$WORK" && "$LIND_COMPILE" --emit-llvm nodebug_callee.c -- -g0 ) >"$WORK/nodebug2.log" 2>&1
out="$(run_llm nodebug nodebug_caller "$WORK/nodebug_caller.bc" "$WORK/nodebug_callee.bc" --)"
base="$(sanitized_base nodebug_caller)"
check "13: exit code" "$(cat "$WORK/nodebug.exitcode")" "0"
check "13: debug-less callee still included" \
    "$(pyjq "$out/$base.prompt.json" "sorted(f['included_functions'])")" \
    "['nodebug_callee', 'nodebug_caller']"
check "13: debug-less callee's arguments appear in the call-edge mapping" \
    "$(grep -c "nodebug_callee\.arg" "$out/$base.prompt.txt")" "2"
check "13: debug-less callee's own arguments carry no parenthetical DWARF name" \
    "$(grep -oE "nodebug_callee\.arg[0-9]+ ?\(" "$out/$base.prompt.txt" | wc -l | tr -d ' ')" \
    "0"
check "13: call-edge argument map still resolves for the debug-less callee" \
    "$(pyjq "$out/$base.prompt.json" "len(f['call_edges'][0]['argument_map'])")" "2"

echo ""
echo "=== entry function itself with no debug info ==="
out="$(run_llm nodebugentry nodebug_callee "$WORK/nodebug_callee.bc" --)"
base="$(sanitized_base nodebug_callee)"
check "entry no debug info: exit code" "$(cat "$WORK/nodebugentry.exitcode")" "0"
check "entry no debug info: plain argN identities" \
    "$(pyjq "$out/$base.prompt.json" "[a['name'] for a in f['entry_arguments']]")" \
    "['arg0', 'arg1']"

echo ""
echo "=== 15. safe handling of an unusual function symbol ==="
# unusual_symbol.ll is hand-written, not compiled via lind_compile: a C
# identifier cannot contain a path separator or a space, so this exact
# shape cannot be expressed as ordinary C source -- see the fixture's own
# comment. parseIRFile accepts .ll text directly, no assembly step needed.
out="$(run_llm unusual "weird/name with spaces" "$SCRIPT_DIR/unusual_symbol.ll" --)"
check "15: exit code" "$(cat "$WORK/unusual.exitcode")" "0"
check "15: artifact written under a sanitized filename" \
    "$([[ -f "$out/weird_name_with_spaces.prompt.json" ]] && echo present || echo absent)" \
    "present"
check "15: no file escaped the output directory" \
    "$(find "$out" -maxdepth 1 -type f | wc -l | tr -d ' ')" "2"
check "15: manifest preserves the exact original symbol" \
    "$(pyjq "$out/weird_name_with_spaces.prompt.json" "f['function']")" \
    "weird/name with spaces"

echo ""
echo "=== 16. ordinary static inference output unchanged when prompt-only mode is absent ==="
plain_json="$WORK/plain.marshal.json"
"$MARSHAL_INFER" --json -o "$plain_json" "$WORK/wrapper.bc" "$WORK/worker.bc" 2>/dev/null
rc=$?
check "16: ordinary inference exit code" "$rc" "0"
check "16: ordinary inference JSON has no llm-prompt fields at all" \
    "$(python3 -c "
import json
d = json.load(open('$plain_json'))
print('response_schema_version' in json.dumps(d) or 'prompt_hash' in json.dumps(d))
")" \
    "False"
check "16: ordinary inference still marshals wrapper_axpy" \
    "$(pyjq "$plain_json" "[fn for fn in f['functions'] if fn['name']=='wrapper_axpy'][0]['decision']")" \
    "marshal"

echo ""
echo "=== 17. CLI validation for missing --function / --prompt-output ==="
"$MARSHAL_INFER" "$WORK/wrapper.bc" --llm-prompt-only --prompt-output "$WORK/out17a" \
    >"$WORK/17a.log" 2>&1
check "17: missing --function: exit code" "$?" "1"
check "17: missing --function: error message" \
    "$(grep -c "requires both --function and --prompt-output" "$WORK/17a.log")" "1"

"$MARSHAL_INFER" "$WORK/wrapper.bc" --llm-prompt-only --function wrapper_axpy \
    >"$WORK/17b.log" 2>&1
check "17: missing --prompt-output: exit code" "$?" "1"
check "17: missing --prompt-output: error message" \
    "$(grep -c "requires both --function and --prompt-output" "$WORK/17b.log")" "1"

"$MARSHAL_INFER" "$WORK/wrapper.bc" --llm-prompt-only --function wrapper_axpy \
    --prompt-output "$WORK/out17c" --llm-max-functions 0 >"$WORK/17c.log" 2>&1
check "17: --llm-max-functions 0: exit code" "$?" "1"
check "17: --llm-max-functions 0: error message" \
    "$(grep -c "must be at least 1" "$WORK/17c.log")" "1"

echo ""
echo "=== 19. pointer passed to an indirect call resolved via a compile-time function-pointer table ==="
if ! compile_llm dispatch_table; then
    echo "  FAIL  19: compile failed"; cat "$WORK/dispatch_table.compile.log"; FAIL=$((FAIL+1))
else
    out="$(run_llm dispatch caller_dispatch_table "$WORK/dispatch_table.bc" --)"
    base="$(sanitized_base caller_dispatch_table)"
    check "19: exit code" "$(cat "$WORK/dispatch.exitcode")" "0"
    check "19: slice_complete true (both candidates have bodies)" \
        "$(pyjq "$out/$base.prompt.json" "f['slice_complete']")" "True"
    check "19: eligible_for_llm_inference true" \
        "$(pyjq "$out/$base.prompt.json" "f['eligible_for_llm_inference']")" "True"
    check "19: both table candidates included" \
        "$(pyjq "$out/$base.prompt.json" "sorted(f['included_functions'])")" \
        "['caller_dispatch_table', 'kernel_a', 'kernel_b']"
    check "19: no incomplete note at all" \
        "$(pyjq "$out/$base.prompt.json" "any(n['kind']=='incomplete' for n in f['notes'])")" \
        "False"
    check "19: an informational note names both candidates" \
        "$(pyjq "$out/$base.prompt.json" "any(n['kind']=='informational' and 'kernel_a' in n['message'] and 'kernel_b' in n['message'] and 'function-pointer table' in n['message'] for n in f['notes'])")" \
        "True"
    check "19: a call edge is recorded to each candidate at the same call site" \
        "$(pyjq "$out/$base.prompt.json" "sorted(e['callee'] for e in f['call_edges'] if e['caller']=='caller_dispatch_table')")" \
        "['kernel_a', 'kernel_b']"
fi

echo ""
echo "=== 20. function-pointer table entry that is declared but never defined ==="
if ! compile_llm table_declaration_only; then
    echo "  FAIL  20: compile failed"; cat "$WORK/table_declaration_only.compile.log"; FAIL=$((FAIL+1))
else
    out="$(run_llm tabledecl caller_table_declaration_only "$WORK/table_declaration_only.bc" --)"
    base="$(sanitized_base caller_table_declaration_only)"
    check "20: exit code" "$(cat "$WORK/tabledecl.exitcode")" "0"
    check "20: slice_complete false" "$(pyjq "$out/$base.prompt.json" "f['slice_complete']")" "False"
    check "20: the real candidate is still included" \
        "$(pyjq "$out/$base.prompt.json" "'table_kernel_ok' in f['included_functions']")" "True"
    check "20: an incomplete note names the missing table entry" \
        "$(pyjq "$out/$base.prompt.json" "any(n['kind']=='incomplete' and 'table_missing_worker' in n['message'] and 'no available body' in n['message'] for n in f['notes'])")" \
        "True"
fi

echo ""
echo "=== 21. function-pointer table entry that resolves ambiguously across resident modules ==="
AMBIG_DIR="$SCRIPT_DIR/ambiguous"
ambig_ok=1
for name in table_ambiguous_caller table_ambig_workerA table_ambig_workerB; do
    cp "$AMBIG_DIR/$name.c" "$WORK/$name.c"
    if ! ( cd "$WORK" && "$LIND_COMPILE" --emit-llvm "$name.c" ) >"$WORK/$name.compile.log" 2>&1; then
        echo "  FAIL  21: $name compile failed"; cat "$WORK/$name.compile.log"; FAIL=$((FAIL+1))
        ambig_ok=0
    fi
done
if [[ "$ambig_ok" -eq 1 ]]; then
    out="$(run_llm tableambig caller_table_ambiguous \
        "$WORK/table_ambiguous_caller.bc" "$WORK/table_ambig_workerA.bc" "$WORK/table_ambig_workerB.bc" --)"
    base="$(sanitized_base caller_table_ambiguous)"
    check "21: exit code" "$(cat "$WORK/tableambig.exitcode")" "0"
    check "21: slice_complete false" "$(pyjq "$out/$base.prompt.json" "f['slice_complete']")" "False"
    check "21: the real candidate is still included" \
        "$(pyjq "$out/$base.prompt.json" "'table_ambig_kernel_ok' in f['included_functions']")" "True"
    check "21: an incomplete note reports the ambiguous table entry" \
        "$(pyjq "$out/$base.prompt.json" "any(n['kind']=='incomplete' and 'table_ambig_shared' in n['message'] and 'ambiguous' in n['message'] for n in f['notes'])")" \
        "True"
fi

echo ""
echo "=== 22. function-pointer table entry that is neither a function nor a provable null ==="
# table_nonfunction_entry.ll is hand-written, not compiled via lind_compile:
# C offers no way to put a non-function, non-null value into a function-
# pointer-typed table entry without a type error -- see the fixture's own
# comment.
out="$(run_llm tablebad caller_table_nonfunction "$SCRIPT_DIR/table_nonfunction_entry.ll" --)"
base="$(sanitized_base caller_table_nonfunction)"
check "22: exit code" "$(cat "$WORK/tablebad.exitcode")" "0"
check "22: slice_complete false" "$(pyjq "$out/$base.prompt.json" "f['slice_complete']")" "False"
check "22: the real candidate is still included" \
    "$(pyjq "$out/$base.prompt.json" "'table_kernel_ok' in f['included_functions']")" "True"
check "22: an incomplete note reports the unresolvable table entry" \
    "$(pyjq "$out/$base.prompt.json" "any(n['kind']=='incomplete' and 'neither a resolvable function' in n['message'] for n in f['notes'])")" \
    "True"

echo ""
echo "=== 18. manifest JSON parses strictly (no duplicate-key-dependent representation) ==="
for f in "$WORK"/out_*/*.prompt.json; do
    [[ -f "$f" ]] || continue
    check "18: $(basename "$(dirname "$f")")/$(basename "$f") parses strictly" \
        "$(strict_json_ok "$f")" "ok"
done

echo ""
echo "Results: $PASS passed, $FAIL failed"
[[ "$FAIL" -eq 0 ]]
