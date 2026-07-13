#!/usr/bin/env python3

import unittest
from unittest.mock import patch

import test_runner


class CategorySelectionTests(unittest.TestCase):
    def test_default_selection_uses_all_categories_in_order(self):
        self.assertEqual(
            test_runner.ordered_categories(None),
            list(test_runner.UNIT_TEST_CATEGORY_ORDER),
        )

    def test_requested_categories_follow_canonical_order(self):
        self.assertEqual(
            test_runner.ordered_categories(
                ["memory", "math", "filesystem"]
            ),
            ["math", "filesystem", "memory"],
        )

    def test_duplicate_categories_are_removed(self):
        self.assertEqual(
            test_runner.ordered_categories(
                ["math", "filesystem", "math"]
            ),
            ["math", "filesystem"],
        )

    def test_category_folders_expands_selected_categories(self):
        self.assertEqual(
            test_runner.category_folders(["math", "filesystem"]),
            [
                "math_tests",
                "file_tests",
            ],
        )

    def test_remaining_categories_after_failure(self):
        self.assertEqual(
            test_runner.remaining_categories(
                ["math", "filesystem", "memory", "process"],
                completed=["math"],
                failed="filesystem",
            ),
            ["memory", "process"],
        )

    def test_no_skipped_categories_after_success(self):
        self.assertEqual(
            test_runner.remaining_categories(
                ["math", "filesystem"],
                completed=["math", "filesystem"],
                failed=None,
            ),
            [],
        )

    def test_report_has_failures_detects_failure_count(self):
        report = {
            "deterministic": {
                "number_of_success": 1,
                "number_of_failures": 2,
            },
            "fail": {
                "number_of_success": 0,
                "number_of_failures": 0,
            },
        }

        self.assertTrue(test_runner.report_has_failures(report))

    def test_report_has_failures_accepts_clean_report(self):
        report = {
            "deterministic": {
                "number_of_success": 3,
                "number_of_failures": 0,
            },
            "libcpp": {
                "number_of_success": 1,
                "number_of_failures": 0,
            },
        }

        self.assertFalse(test_runner.report_has_failures(report))
    def test_build_wasm_category_summary(self):
        category_results = [
            {
                "name": "wasm-math",
                "report": {
                    "deterministic": {
                        "number_of_failures": 0,
                    }
                },
            },
            {
                "name": "wasm-filesystem",
                "report": {
                    "deterministic": {
                        "number_of_failures": 2,
                    }
                },
            },
        ]

        summary = test_runner.build_wasm_category_summary(
            category_results,
            failed_category="filesystem",
            skipped_categories=["memory"],
        )

        self.assertEqual(summary["number_of_failures"], 2)
        self.assertEqual(summary["completed_categories"], ["math"])
        self.assertEqual(summary["failed_category"], "filesystem")
        self.assertEqual(summary["skipped_categories"], ["memory"])
        self.assertEqual(
            set(summary["categories"]),
            {"math", "filesystem"},
        )

class CategoryExecutionTests(unittest.TestCase):
    @staticmethod
    def _successful_result():
        return {
            "name": "wasm",
            "json_filename": "wasm.json",
            "html_filename": "report.html",
            "report": {
                "deterministic": {
                    "number_of_success": 1,
                    "number_of_failures": 0,
                }
            },
            "html": "<html></html>",
        }

    @patch("test_runner.run_harness")
    def test_staged_execution_runs_categories_separately(
        self,
        mock_run_harness,
    ):
        mock_run_harness.side_effect = [
            self._successful_result(),
            self._successful_result(),
        ]

        results, failed, skipped = test_runner.run_wasm_categories(
            ["math", "filesystem"],
            ["--timeout", "30"],
            staged=True,
        )

        self.assertEqual(len(results), 2)
        self.assertIsNone(failed)
        self.assertEqual(skipped, [])
        self.assertEqual(mock_run_harness.call_count, 2)

        self.assertEqual(
            mock_run_harness.call_args_list[0].args,
            (
                "wasmtestreport",
                [
                    "--timeout",
                    "30",
                    "--allow-pre-compiled",
                    "--skip-libcpp",
                    "--skip",
                    "static_tests",
                    "--run",
                    "math_tests",
                ],
            ),
        )

        self.assertEqual(
            mock_run_harness.call_args_list[1].args,
            (
                "wasmtestreport",
                [
                    "--timeout",
                    "30",
                    "--allow-pre-compiled",
                    "--skip-libcpp",
                    "--skip",
                    "static_tests",
                    "--run",
                    "file_tests",
                ],
            ),
        )

    @patch("test_runner.run_harness")
    def test_staged_execution_stops_after_failure(
        self,
        mock_run_harness,
    ):
        mock_run_harness.side_effect = [
            self._successful_result(),
            RuntimeError("filesystem failed"),
        ]

        results, failed, skipped = test_runner.run_wasm_categories(
            ["math", "filesystem", "memory", "process"],
            [],
            staged=True,
        )

        self.assertEqual(len(results), 1)
        self.assertEqual(failed, "filesystem")
        self.assertEqual(skipped, ["memory", "process"])
        self.assertEqual(mock_run_harness.call_count, 2)

    @patch("test_runner.run_harness")
    def test_staged_execution_stops_on_reported_failure(
        self,
        mock_run_harness,
    ):
        successful = self._successful_result()
        failed_result = self._successful_result()
        failed_result["report"]["deterministic"]["number_of_failures"] = 2

        mock_run_harness.side_effect = [
            successful,
            failed_result,
        ]

        results, failed, skipped = test_runner.run_wasm_categories(
            ["math", "filesystem", "memory"],
            [],
            staged=True,
        )

        self.assertEqual(len(results), 2)
        self.assertEqual(failed, "filesystem")
        self.assertEqual(skipped, ["memory"])
        self.assertEqual(mock_run_harness.call_count, 2)

    @patch("test_runner.run_harness")
    def test_non_staged_execution_combines_categories(
        self,
        mock_run_harness,
    ):
        mock_run_harness.return_value = self._successful_result()

        results, failed, skipped = test_runner.run_wasm_categories(
            ["math", "filesystem"],
            [],
            staged=False,
        )

        self.assertEqual(len(results), 1)
        self.assertIsNone(failed)
        self.assertEqual(skipped, [])
        self.assertEqual(mock_run_harness.call_count, 1)

        self.assertEqual(
            mock_run_harness.call_args.args,
            (
                "wasmtestreport",
                [
                    "--allow-pre-compiled",
                    "--skip-libcpp",
                    "--skip",
                    "static_tests",
                    "--run",
                    "math_tests",
                    "file_tests",
                ],
            ),
        )


if __name__ == "__main__":
    unittest.main()
