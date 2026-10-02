#!/usr/bin/env python3
"""Unit tests for execution_oracle.py (issue #22's OpenBLAS inference-
to-runtime integration, Gate 5: "strict execution oracle").

Covers parse_call_counts/parse_ptr_sizes against real-shaped
[lind-trace] output, and classify()'s own 8-way taxonomy decision --
including the gate's own explicit acceptance criterion: a numerically
correct run with no required symbol observed dispatched must classify
as PASS_LOCAL_ONLY, never PASS_INTERPOSED.
"""
import os
import sys
import unittest

_TOOLS_DIR = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
sys.path.insert(0, os.path.join(_TOOLS_DIR, "marshal-gen"))
import execution_oracle as oracle  # noqa: E402


class ParseCallCountsTests(unittest.TestCase):
    def test_single_call(self):
        out = "some noise\n[lind-trace] call cblas_daxpy\nmore noise\n"
        self.assertEqual(oracle.parse_call_counts(out), {"cblas_daxpy": 1})

    def test_repeated_calls_are_counted(self):
        out = "\n".join(["[lind-trace] call cblas_daxpy"] * 3)
        self.assertEqual(oracle.parse_call_counts(out), {"cblas_daxpy": 3})

    def test_multiple_distinct_symbols(self):
        out = "[lind-trace] call cblas_daxpy\n[lind-trace] call cblas_dscal\n[lind-trace] call cblas_daxpy\n"
        self.assertEqual(oracle.parse_call_counts(out), {"cblas_daxpy": 2, "cblas_dscal": 1})

    def test_no_trace_lines_yields_empty_dict(self):
        self.assertEqual(oracle.parse_call_counts("[Cage|foo] PASS: bar\napp exited 0\n"), {})

    def test_unrelated_lines_mentioning_call_are_not_matched(self):
        # Must anchor on the EXACT "[lind-trace] call " prefix, not just
        # any line containing the word "call" somewhere.
        out = "[Grate|foo] registered 1/1 handlers\nthis test will call cblas_daxpy next\n"
        self.assertEqual(oracle.parse_call_counts(out), {})


class ParsePtrSizesTests(unittest.TestCase):
    def test_single_ptr_line(self):
        out = "[lind-trace] cblas_dnrm2 ptr size_kind=stride_vector bytes=0x28\n"
        self.assertEqual(oracle.parse_ptr_sizes(out), [("cblas_dnrm2", "stride_vector", 0x28)])

    def test_order_is_preserved(self):
        out = ("[lind-trace] cblas_daxpy ptr size_kind=stride_vector bytes=0x8\n"
               "[lind-trace] cblas_daxpy ptr size_kind=stride_vector bytes=0x10\n")
        self.assertEqual(oracle.parse_ptr_sizes(out),
                          [("cblas_daxpy", "stride_vector", 0x8), ("cblas_daxpy", "stride_vector", 0x10)])

    def test_call_lines_are_not_mistaken_for_ptr_lines(self):
        out = "[lind-trace] call cblas_daxpy\n"
        self.assertEqual(oracle.parse_ptr_sizes(out), [])


class ClassifyTests(unittest.TestCase):
    def test_pass_interposed_when_required_symbol_observed(self):
        result = oracle.classify(required_symbols=("cblas_daxpy",), numeric_ok=True,
                                  call_counts={"cblas_daxpy": 1})
        self.assertEqual(result, oracle.PASS_INTERPOSED)

    def test_pass_local_only_when_numerically_correct_but_symbol_never_observed(self):
        # The gate's own explicit acceptance criterion: a numeric pass
        # with registration disabled (so call_counts is empty/doesn't
        # mention the symbol) must be PASS_LOCAL_ONLY, not PASS_INTERPOSED.
        result = oracle.classify(required_symbols=("cblas_daxpy",), numeric_ok=True, call_counts={})
        self.assertEqual(result, oracle.PASS_LOCAL_ONLY)

    def test_pass_local_only_when_a_different_symbol_was_observed(self):
        # Evidence must name the SPECIFIC required symbol, not just
        # "some dispatch happened somewhere in this run."
        result = oracle.classify(required_symbols=("cblas_daxpy",), numeric_ok=True,
                                  call_counts={"cblas_dscal": 5})
        self.assertEqual(result, oracle.PASS_LOCAL_ONLY)

    def test_pass_interposed_if_any_one_of_several_required_symbols_observed(self):
        result = oracle.classify(required_symbols=("cblas_daxpy", "cblas_dscal"), numeric_ok=True,
                                  call_counts={"cblas_dscal": 1})
        self.assertEqual(result, oracle.PASS_INTERPOSED)

    def test_fail_numeric_when_wrong_result_and_no_baseline_info(self):
        result = oracle.classify(required_symbols=("cblas_daxpy",), numeric_ok=False)
        self.assertEqual(result, oracle.FAIL_NUMERIC)

    def test_baseline_fail_when_wrong_result_and_baseline_also_failed(self):
        result = oracle.classify(required_symbols=("cblas_daxpy",), numeric_ok=False, baseline_failed=True)
        self.assertEqual(result, oracle.BASELINE_FAIL)

    def test_fail_marshal_on_grate_err_sentinel_rejection(self):
        result = oracle.classify(required_symbols=("cblas_daxpy",), numeric_ok=False,
                                  rejected_with_sentinel=True)
        self.assertEqual(result, oracle.FAIL_MARSHAL)

    def test_trap_takes_priority_over_a_sentinel_or_numeric_check(self):
        result = oracle.classify(required_symbols=("cblas_daxpy",), numeric_ok=False,
                                  trapped=True, rejected_with_sentinel=True)
        self.assertEqual(result, oracle.TRAP)

    def test_fail_abi_on_build_failure(self):
        result = oracle.classify(required_symbols=("cblas_daxpy",), numeric_ok=False, build_failed=True)
        self.assertEqual(result, oracle.FAIL_ABI)

    def test_fail_abi_on_registration_failure(self):
        result = oracle.classify(required_symbols=("cblas_daxpy",), numeric_ok=False, registration_failed=True)
        self.assertEqual(result, oracle.FAIL_ABI)

    def test_unsupported_short_circuits_everything_else(self):
        result = oracle.classify(required_symbols=("cblas_daxpy",), numeric_ok=True,
                                  call_counts={"cblas_daxpy": 1}, unsupported=True)
        self.assertEqual(result, oracle.UNSUPPORTED)

    def test_build_failed_takes_priority_over_unsupported_is_false_by_default(self):
        # unsupported is checked FIRST among the failure-priority checks
        # -- confirms the explicit ordering, not an accidental one.
        result = oracle.classify(required_symbols=("cblas_daxpy",), numeric_ok=False,
                                  build_failed=True, unsupported=True)
        self.assertEqual(result, oracle.UNSUPPORTED)


if __name__ == "__main__":
    unittest.main()
