#!/usr/bin/env python3
"""Gate 5's "strict execution oracle" -- issue #22's OpenBLAS inference-
to-runtime integration.

A test that merely produces the right NUMERIC answer never proves a real
symbol actually crossed the grate: a silent fallback to the real,
uninterposed library can produce the identical result (PASS_LOCAL_ONLY,
not PASS_INTERPOSED). This module provides the two pieces a harness needs
to tell those apart and classify a test's real outcome:

  - parse_call_counts/parse_ptr_sizes: parse the `[lind-trace] ...` lines
    lind_marshal.h's own debug-trace hooks print when a grate is compiled
    with -DLIND_MARSHAL_DEBUG (see that header's own doc) -- the actual
    EVIDENCE a symbol was dispatched through the grate, and what byte
    count each marshalled pointer argument was given.
  - classify: maps a test's own already-known facts (did it build, did
    registration succeed, did it trap, did it reject with the
    LIND_GRATE_ERR sentinel, did its own assertions pass, which real
    symbols were actually observed dispatched) onto the plan's own 8-way
    taxonomy. Deliberately takes these as STRUCTURED booleans/dicts
    rather than trying to regex-guess "was this a marshal failure or an
    ABI failure" from free-text output -- a harness integration (e.g.
    run_tests.sh's own existing category_for/decide_outcome conventions)
    already has these facts directly from its own build/registration/run
    phases, and guessing them from text here would be exactly the kind
    of unsound inference this whole engagement has consistently rejected
    elsewhere.

Taxonomy (see local-notes/active/plan-openblas-inference-runtime-integration.md's
own Gate 5 text):
  PASS_INTERPOSED   expected result AND required symbol(s) intercepted
  PASS_LOCAL_ONLY   numerical pass but no required interposed call
  FAIL_NUMERIC      completed with wrong result
  FAIL_MARSHAL      bounds/copy/expression/translation failure
  FAIL_ABI          signature or V2 transport failure
  TRAP              process/runtime trap
  UNSUPPORTED       required symbol intentionally local
  BASELINE_FAIL     fails without interposition too
"""
import re

PASS_INTERPOSED = "PASS_INTERPOSED"
PASS_LOCAL_ONLY = "PASS_LOCAL_ONLY"
FAIL_NUMERIC = "FAIL_NUMERIC"
FAIL_MARSHAL = "FAIL_MARSHAL"
FAIL_ABI = "FAIL_ABI"
TRAP = "TRAP"
UNSUPPORTED = "UNSUPPORTED"
BASELINE_FAIL = "BASELINE_FAIL"

ALL_OUTCOMES = (PASS_INTERPOSED, PASS_LOCAL_ONLY, FAIL_NUMERIC, FAIL_MARSHAL,
                FAIL_ABI, TRAP, UNSUPPORTED, BASELINE_FAIL)

_TRACE_CALL_RE = re.compile(r"\[lind-trace\] call (\S+)")
_TRACE_PTR_RE = re.compile(r"\[lind-trace\] (\S+) ptr size_kind=(\S+) bytes=0x([0-9a-f]+)")


def parse_call_counts(output):
    """Returns {symbol: call_count}, parsed from every `[lind-trace] call
    <symbol>` line lind_marshal.h's own _lind_dbg_call prints (once per
    dispatched call, for both V1's lind_marshal_dispatch and every V2
    generated adapter alike) -- the per-symbol call-count evidence
    Gate 5's own text asks for "at minimum"."""
    counts = {}
    for m in _TRACE_CALL_RE.finditer(output):
        counts[m.group(1)] = counts.get(m.group(1), 0) + 1
    return counts


def parse_ptr_sizes(output):
    """Returns [(symbol, size_kind, bytes), ...] in call order, parsed
    from every `[lind-trace] <symbol> ptr size_kind=<kind> bytes=0x<hex>`
    line _lind_dbg_ptr_size prints -- the "preferably also" selected-
    contract-source/computed-byte-count evidence."""
    return [(m.group(1), m.group(2), int(m.group(3), 16)) for m in _TRACE_PTR_RE.finditer(output)]


def classify(*, required_symbols, numeric_ok,
             build_failed=False, registration_failed=False, trapped=False,
             rejected_with_sentinel=False, unsupported=False, baseline_failed=False,
             call_counts=None):
    """Classifies one test's real outcome per the 8-way taxonomy above.

    `required_symbols`: the real library symbol(s) this test's own claim
    of interposed coverage depends on, e.g. ("cblas_daxpy",).
    `numeric_ok`: whether the test's own assertions (its expected result)
    passed -- independent of whether that result came from a real
    dispatch or a local fallback; that distinction is `call_counts`'s job.
    `build_failed`/`registration_failed`: a compile-time or
    register_lib_handler(_v2) failure -- always FAIL_ABI, since neither
    can reach numeric_ok or a dispatch at all.
    `trapped`: a raw process trap/signal with no coherent result --
    always TRAP, checked before any other failure classification.
    `rejected_with_sentinel`: the call returned the established
    LIND_GRATE_ERR sentinel (a marshalling contract violation
    _lind_marshal_abort caught and converted to a caller-visible
    rejection, not a process crash) -- FAIL_MARSHAL.
    `unsupported`: the caller already knows (e.g. from Gate 2/3's own
    import report) that the required symbol was deliberately left
    local/force_local for a documented capability reason -- short-
    circuits straight to UNSUPPORTED regardless of any other argument.
    `baseline_failed`: this SAME test's own non-interposed baseline also
    failed -- a numeric failure that fails either way is BASELINE_FAIL,
    not a regression this transport introduced.
    `call_counts`: {symbol: count}, typically parse_call_counts()'s own
    output -- PASS_INTERPOSED requires at least one required symbol to
    have a nonzero count; PASS_LOCAL_ONLY is deliberately never promoted
    to PASS_INTERPOSED just because the test's own assertions passed.
    """
    if unsupported:
        return UNSUPPORTED
    if build_failed or registration_failed:
        return FAIL_ABI
    if trapped:
        return TRAP
    if rejected_with_sentinel:
        return FAIL_MARSHAL
    if not numeric_ok:
        return BASELINE_FAIL if baseline_failed else FAIL_NUMERIC

    call_counts = call_counts or {}
    interposed = any(call_counts.get(sym, 0) > 0 for sym in required_symbols)
    return PASS_INTERPOSED if interposed else PASS_LOCAL_ONLY
