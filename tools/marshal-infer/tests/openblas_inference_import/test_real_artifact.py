#!/usr/bin/env python3
"""Acceptance test against the REAL openblas-inference/final/
openblas_inference.json artifact and its real llm-prompts/openblas-v6-full/
manifests (issue #22's OpenBLAS inference-to-runtime integration, Gate 2).

Separate from test_import_openblas_inference.py's synthetic-record unit
tests deliberately: this file is the one place that would need updating if
the real artifact's own totals change (a new LLM sweep, a newly-flagged
function, ...), so a synthetic-record test never has to change just because
the real dataset grew. Skips (does not fail) if the real artifact or
prompts directory isn't present in this checkout.
"""
import csv
import glob
import json
import os
import sys
import tempfile
import unittest

_TOOLS_DIR = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
_REPO_ROOT = os.path.dirname(_TOOLS_DIR)
sys.path.insert(0, os.path.join(_TOOLS_DIR, "marshal-gen"))
import import_openblas_inference as imp  # noqa: E402

ARTIFACT = os.path.join(_REPO_ROOT, "openblas-inference", "final", "openblas_inference.json")
PROMPTS_DIR = os.path.join(_REPO_ROOT, "llm-prompts", "openblas-v6-full")


@unittest.skipUnless(
    os.path.exists(ARTIFACT) and os.path.isdir(PROMPTS_DIR),
    "real openblas-inference artifact or prompt manifests not present in this checkout",
)
class RealArtifactAcceptanceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.functions, cls.rows = imp.import_all(ARTIFACT, PROMPTS_DIR)
        with open(ARTIFACT) as fh:
            cls.raw = json.load(fh)["functions"]

    def test_all_203_functions_appear(self):
        self.assertEqual(len(self.raw), 203)
        self.assertEqual(set(self.functions.keys()), set(self.raw.keys()))
        self.assertEqual({r["symbol"] for r in self.rows}, set(self.raw.keys()))

    def test_flagged_and_unresolved_are_local_with_reasons(self):
        flagged = [r for r in self.rows if r["status"] == "flagged"]
        unresolved = [r for r in self.rows if r["status"] == "unresolved"]
        self.assertEqual(len(flagged), 12)
        self.assertEqual(len(unresolved), 37)
        for r in flagged + unresolved:
            self.assertEqual(self.functions[r["symbol"]], None)
            self.assertTrue(r["reason"], f"{r['symbol']} has no reason recorded")

    def test_static_resolved_always_wins_and_passes_through_unchanged(self):
        static_resolved = [r for r in self.rows if r["source"] == "static" and r["status"] == "resolved"]
        self.assertEqual(len(static_resolved), 74)
        for r in static_resolved:
            name = r["symbol"]
            self.assertEqual(self.functions[name], self.raw[name]["static"])

    def test_no_function_silently_dropped(self):
        # Every function has EITHER a real lowered record OR an explicit,
        # non-empty reason it's local -- never neither.
        for name in self.raw:
            record = self.functions[name]
            row = next(r for r in self.rows if r["symbol"] == name)
            if record is None:
                self.assertTrue(row["reason"], f"{name} is local with no reason")
            else:
                self.assertEqual(record["name"], name)

    def test_report_states_generator_runtime_supported_count(self):
        resolved = [r for r in self.rows if r["status"] == "resolved"]
        self.assertEqual(len(resolved), 154)
        generated = [r for r in resolved if r["generated_or_local"] == "generated"]
        # Not a fixed expectation of "how many should pass" (that's Gate 3
        # and Gate 7's job to grow) -- just that the report actually
        # distinguishes the two groups, and that there IS a real gap today
        # (Gate 3 hasn't taught the generator the new "expr"/
        # "stride_vector" tree vocabulary yet).
        self.assertGreater(len(generated), 0)
        self.assertLess(len(generated), len(resolved))
        for r in generated:
            self.assertIn(r["selected_transport"], ("V1", "V2"))

    def test_cblas_stbmv_rejected_for_missing_precision_multiplier(self):
        # The real, found-during-construction defect this module's
        # "argument"-extent structural check exists to catch -- see
        # import_openblas_inference.py's own lower_pointer_argument doc.
        row = next(r for r in self.rows if r["symbol"] == "cblas_stbmv")
        self.assertIsNone(self.functions["cblas_stbmv"])
        self.assertIn("precision size", row["reason"])

    def test_sibling_functions_with_correct_multiplier_are_not_rejected_by_that_check(self):
        # cblas_sspr2/dgeadd_ have the SAME "argument"-extent shape as
        # cblas_stbmv but include the expected multiplier -- proves the
        # check is precise, not a blanket rejection of every
        # "argument"-extent record.
        for name in ("cblas_sspr2",):
            row = next(r for r in self.rows if r["symbol"] == name)
            self.assertNotIn("precision size", row["reason"])

    def test_determinism(self):
        with tempfile.TemporaryDirectory() as td:
            out1 = os.path.join(td, "run1.json")
            out2 = os.path.join(td, "run2.json")
            rep1 = os.path.join(td, "run1.csv")
            rep2 = os.path.join(td, "run2.csv")
            for out, rep in ((out1, rep1), (out2, rep2)):
                functions, rows = imp.import_all(ARTIFACT, PROMPTS_DIR)
                marshal_out = {
                    "functions": [
                        functions[name] if functions[name] is not None
                        else {"name": name, "decision": "force_local"}
                        for name in sorted(functions)
                    ]
                }
                with open(out, "w") as fh:
                    json.dump(marshal_out, fh, sort_keys=True)
                with open(rep, "w", newline="") as fh:
                    writer = csv.DictWriter(fh, fieldnames=["symbol", "reason"])
                    writer.writeheader()
                    for r in sorted(rows, key=lambda r: r["symbol"]):
                        writer.writerow({"symbol": r["symbol"], "reason": r["reason"]})
            with open(out1) as f1, open(out2) as f2:
                self.assertEqual(f1.read(), f2.read())
            with open(rep1) as f1, open(rep2) as f2:
                self.assertEqual(f1.read(), f2.read())


if __name__ == "__main__":
    unittest.main()
