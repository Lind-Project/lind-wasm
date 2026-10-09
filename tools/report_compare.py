#!/usr/bin/env python3
"""
Interactive CLI for comparing lind-wasm test reports (reports/<name>/wasm.json).

Lets you pick two report runs (baseline vs current) and:
  - see per-category (error_type) failure breakdowns
  - see what newly broke / newly got fixed / changed category
  - drill into a single test's output in both runs
  - define your own regex-based buckets (e.g. `bucket add missing_handler
    "no handler for syscall_num"`) that persist across runs in
    ~/.config/lind-wasm/report_compare_buckets.json

Usage:
    python3 tools/report_compare.py                 # interactive REPL
    python3 tools/report_compare.py --a 09-12 --b 09-16-with-pth_crt
    python3 tools/report_compare.py --a 09-12 --b 09-16-with-pth_crt --diff-only
"""
from __future__ import annotations

import argparse
import cmd
import json
import os
import re
import shlex
import sys
import textwrap
from dataclasses import dataclass, field
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
REPORTS_DIR = REPO_ROOT / "reports"
REPORT_FILENAME = "wasm.json"

# User-level (not repo-level) so custom buckets survive across clones/runs.
CONFIG_DIR = Path(os.environ.get("XDG_CONFIG_HOME", str(Path.home() / ".config"))) / "lind-wasm"
BUCKETS_FILE = CONFIG_DIR / "report_compare_buckets.json"

# Strip the volatile /tmp/wasmtest_artifacts_XXXX/ prefix so test names
# line up across runs.
_ARTIFACT_RE = re.compile(r".*wasmtest_artifacts_[^/]+/")


def normalize_name(path: str) -> str:
    return _ARTIFACT_RE.sub("", path)


def load_buckets() -> dict[str, str]:
    """Load saved {name: regex} buckets, e.g. for grouping the many
    '[3i|_get_handler] no handler for syscall_num: N' failures together."""
    if not BUCKETS_FILE.exists():
        return {}
    try:
        with open(BUCKETS_FILE, encoding="utf-8") as f:
            return json.load(f)
    except (json.JSONDecodeError, OSError):
        return {}


def save_buckets(buckets: dict[str, str]) -> None:
    CONFIG_DIR.mkdir(parents=True, exist_ok=True)
    with open(BUCKETS_FILE, "w", encoding="utf-8") as f:
        json.dump(buckets, f, indent=2, sort_keys=True)


@dataclass
class TestCase:
    suite: str
    name: str
    status: str
    error_type: str | None
    output: str

    @property
    def category(self) -> str:
        return "Success" if self.status == "Success" else (self.error_type or "Unknown_Failure")


@dataclass
class Report:
    label: str
    path: Path
    tests: dict[str, TestCase] = field(default_factory=dict)

    @classmethod
    def load(cls, path: Path) -> "Report":
        with open(path, encoding="utf-8") as f:
            raw = json.load(f)
        tests: dict[str, TestCase] = {}
        for suite_name, suite_data in raw.items():
            if not isinstance(suite_data, dict):
                continue
            cases = suite_data.get("test_cases")
            if not isinstance(cases, dict):
                continue
            for raw_name, case in cases.items():
                if not isinstance(case, dict):
                    continue
                name = normalize_name(raw_name)
                tests[f"{suite_name}/{name}"] = TestCase(
                    suite=suite_name,
                    name=name,
                    status=case.get("status", "Unknown"),
                    error_type=case.get("error_type"),
                    output=case.get("output", "") or "",
                )
        return cls(label=path.parent.name, path=path, tests=tests)

    def category_counts(self) -> dict[str, int]:
        counts: dict[str, int] = {}
        for t in self.tests.values():
            counts[t.category] = counts.get(t.category, 0) + 1
        return counts


def discover_reports(reports_dir: Path = REPORTS_DIR) -> list[Path]:
    return sorted(reports_dir.glob(f"*/{REPORT_FILENAME}"))


def find_report(token: str, candidates: list[Path]) -> Path | None:
    """Resolve a user-provided token (index, folder name substring, or path) to a report file."""
    if token.isdigit():
        idx = int(token)
        if 0 <= idx < len(candidates):
            return candidates[idx]
        return None
    # exact / substring match on folder name
    matches = [p for p in candidates if token == p.parent.name]
    if not matches:
        matches = [p for p in candidates if token in p.parent.name]
    if len(matches) == 1:
        return matches[0]
    direct = Path(token)
    if direct.exists():
        return direct
    return None


def wrap(text: str, width: int = 100, indent: str = "    ") -> str:
    lines = text.splitlines() or [""]
    out = []
    for line in lines[:20]:
        out.append(textwrap.fill(line, width=width, initial_indent=indent, subsequent_indent=indent))
    if len(lines) > 20:
        out.append(f"{indent}... ({len(lines) - 20} more lines truncated)")
    return "\n".join(out)


class Diff:
    """Computes the delta between two reports (a=baseline, b=current)."""

    def __init__(self, a: Report, b: Report):
        self.a = a
        self.b = b
        names = set(a.tests) | set(b.tests)
        self.only_in_a = sorted(n for n in names if n not in b.tests)
        self.only_in_b = sorted(n for n in names if n not in a.tests)
        self.newly_failing: list[str] = []
        self.newly_fixed: list[str] = []
        self.still_failing_same_cat: list[str] = []
        self.still_failing_changed_cat: list[str] = []
        self.still_passing: list[str] = []
        for n in sorted(names & set(a.tests) & set(b.tests)):
            ta, tb = a.tests[n], b.tests[n]
            a_fail, b_fail = ta.status != "Success", tb.status != "Success"
            if not a_fail and not b_fail:
                self.still_passing.append(n)
            elif not a_fail and b_fail:
                self.newly_failing.append(n)
            elif a_fail and not b_fail:
                self.newly_fixed.append(n)
            elif ta.category == tb.category:
                self.still_failing_same_cat.append(n)
            else:
                self.still_failing_changed_cat.append(n)

    def summary(self) -> str:
        lines = [
            f"Baseline: {self.a.label} ({len(self.a.tests)} tests)",
            f"Current : {self.b.label} ({len(self.b.tests)} tests)",
            "",
            f"  newly failing        : {len(self.newly_failing)}",
            f"  newly fixed          : {len(self.newly_fixed)}",
            f"  still failing (same) : {len(self.still_failing_same_cat)}",
            f"  still failing (diff category): {len(self.still_failing_changed_cat)}",
            f"  still passing        : {len(self.still_passing)}",
            f"  only in baseline      : {len(self.only_in_a)}",
            f"  only in current       : {len(self.only_in_b)}",
        ]
        return "\n".join(lines)


BUCKETS = [
    "newly_failing",
    "newly_fixed",
    "still_failing_same_cat",
    "still_failing_changed_cat",
    "only_in_a",
    "only_in_b",
]


class ReportShell(cmd.Cmd):
    intro = "lind-wasm report comparator. Type `help` for commands, `reports` to list runs.\n"
    prompt = "(reports) "

    def __init__(self, reports_dir: Path = REPORTS_DIR):
        super().__init__()
        self.reports_dir = reports_dir
        self.candidates = discover_reports(reports_dir)
        self.a: Report | None = None
        self.b: Report | None = None
        self.diff: Diff | None = None
        self._last_list: list[str] = []
        self.custom_buckets: dict[str, str] = load_buckets()

    def preloop(self):
        # default readline delims include '-', which would split report
        # labels like "09-16-with-pth_crt" mid-word and break completion.
        try:
            import readline

            readline.set_completer_delims(" \t\n")
        except ImportError:
            pass

    # ---- helpers -----------------------------------------------------
    def _refresh_diff(self):
        self.diff = Diff(self.a, self.b) if self.a and self.b else None

    def _require(self, *, both=False) -> bool:
        if both and not (self.a and self.b):
            print("Select two reports first: `use <a> <b>`")
            return False
        if not both and not self.a:
            print("Select at least one report first: `use <a>`")
            return False
        return True

    def _matching_bucket(self, pattern: str) -> list[str]:
        rx = re.compile(pattern)
        matched = set()
        for report in (self.a, self.b):
            if not report:
                continue
            for n, t in report.tests.items():
                if t.status != "Success" and rx.search(t.output):
                    matched.add(n)
        return sorted(matched)

    def _resolve_bucket(self, key: str) -> list[str] | None:
        """Resolve a diff bucket / failure category / custom regex bucket name
        to its list of test names, or None (after printing an error) if the
        required reports aren't selected yet."""
        if key in BUCKETS:
            if not self._require(both=True):
                return None
            return list(getattr(self.diff, key))
        if key in self.custom_buckets:
            if not self._require():
                return None
            return self._matching_bucket(self.custom_buckets[key])
        if not self._require():
            return None
        report = self.b or self.a
        return sorted(n for n, t in report.tests.items() if t.category == key)

    @staticmethod
    def _complete_from(text: str, options) -> list[str]:
        return [o for o in options if o.startswith(text)]

    def _report_labels(self) -> list[str]:
        self.candidates = discover_reports(self.reports_dir)
        return [p.parent.name for p in self.candidates]

    def _category_options(self) -> list[str]:
        cats: set[str] = set()
        for r in (self.a, self.b):
            if r:
                cats.update(r.category_counts())
        return sorted(cats)

    def _suite_options(self) -> list[str]:
        suites: set[str] = set()
        for r in (self.a, self.b):
            if r:
                suites.update(t.suite for t in r.tests.values())
        return sorted(suites)

    # ---- commands ------------------------------------------------------
    def do_reports(self, _arg):
        "List discovered report runs (reports/<name>/wasm.json)."
        self.candidates = discover_reports(self.reports_dir)
        for i, p in enumerate(self.candidates):
            marker = ""
            if self.a and self.a.path == p:
                marker += " [a]"
            if self.b and self.b.path == p:
                marker += " [b]"
            print(f"  {i:2d}  {p.parent.name}{marker}")

    def do_use(self, arg):
        "use <a> [<b>]  -- select baseline (a) and optionally current (b) report by index or folder name."
        tokens = arg.split()
        if not tokens:
            print("usage: use <a> [<b>]")
            return
        pa = find_report(tokens[0], self.candidates)
        if not pa:
            print(f"could not resolve report: {tokens[0]}")
            return
        self.a = Report.load(pa)
        self.b = None
        self.diff = None
        print(f"a = {self.a.label} ({len(self.a.tests)} tests)")
        if len(tokens) > 1:
            pb = find_report(tokens[1], self.candidates)
            if not pb:
                print(f"could not resolve report: {tokens[1]}")
                return
            self.b = Report.load(pb)
            print(f"b = {self.b.label} ({len(self.b.tests)} tests)")
            self._refresh_diff()

    def complete_use(self, text, line, begidx, endidx):
        return self._complete_from(text, self._report_labels())

    def do_summary(self, _arg):
        "Show category counts for the selected report(s), and the diff summary if two are selected."
        if not self._require():
            return
        print(f"--- {self.a.label} ---")
        for cat, count in sorted(self.a.category_counts().items(), key=lambda kv: -kv[1]):
            print(f"  {count:4d}  {cat}")
        if self.b:
            print(f"\n--- {self.b.label} ---")
            for cat, count in sorted(self.b.category_counts().items(), key=lambda kv: -kv[1]):
                print(f"  {count:4d}  {cat}")
            print()
            print(self.diff.summary())
        if self.custom_buckets:
            print("\n--- custom buckets ---")
            for name, pattern in self.custom_buckets.items():
                try:
                    rx = re.compile(pattern)
                except re.error:
                    print(f"  {name}: <invalid regex: {pattern}>")
                    continue
                count_a = sum(1 for t in self.a.tests.values() if t.status != "Success" and rx.search(t.output))
                line = f"  {name}: a={count_a}"
                if self.b:
                    count_b = sum(1 for t in self.b.tests.values() if t.status != "Success" and rx.search(t.output))
                    line += f" b={count_b}"
                print(line)

    def do_list(self, arg):
        """list <bucket|category|custom-bucket> [suite-substring]
        list <bucketA> not|and|or <bucketB> [suite-substring]
        List test names matching a diff bucket, failure category, or saved regex
        bucket (see `bucket`). Combine two with a set op, e.g.:
            list UndefSyscall not Lind_wasm_Timeout
            list Unknown_Failure and newly_failing"""
        tokens = arg.split()
        if not tokens:
            print(f"buckets: {', '.join(BUCKETS)}")
            if self.custom_buckets:
                print(f"custom buckets: {', '.join(self.custom_buckets)}")
            print("or a category name, e.g. Unknown_Failure, Output_mismatch, Lind_wasm_Timeout")
            return
        key = tokens[0]
        if len(tokens) >= 3 and tokens[1] in ("not", "and", "or"):
            op, key2 = tokens[1], tokens[2]
            suite_filter = tokens[3] if len(tokens) > 3 else None
            names1, names2 = self._resolve_bucket(key), self._resolve_bucket(key2)
            if names1 is None or names2 is None:
                return
            set1, set2 = set(names1), set(names2)
            combined = {"not": set1 - set2, "and": set1 & set2, "or": set1 | set2}[op]
            names = sorted(combined)
        else:
            suite_filter = tokens[1] if len(tokens) > 1 else None
            names = self._resolve_bucket(key)
            if names is None:
                return
        if suite_filter:
            names = [n for n in names if suite_filter in n]
        self._last_list = names
        for i, n in enumerate(names):
            print(f"  {i:3d}  {n}")
        print(f"{len(names)} test(s)")

    def complete_list(self, text, line, begidx, endidx):
        args = line[:begidx].split()
        bucket_options = list(BUCKETS) + list(self.custom_buckets) + self._category_options()
        if len(args) <= 1:  # completing the first bucket/category argument
            return self._complete_from(text, bucket_options)
        if len(args) == 2:  # after the first bucket: a set op or a suite filter
            return self._complete_from(text, ["not", "and", "or"] + self._suite_options())
        if len(args) == 3 and args[2] in ("not", "and", "or"):
            return self._complete_from(text, bucket_options)
        return self._complete_from(text, self._suite_options())

    def do_show(self, arg):
        "show <name-or-index> -- show status/output for a test in both selected reports (index refers to last `list`)."
        arg = arg.strip()
        if not arg:
            print("usage: show <test-name-or-index-from-last-list>")
            return
        if arg.isdigit() and int(arg) < len(self._last_list):
            name = self._last_list[int(arg)]
        else:
            name = arg
        found = False
        for tag, report in (("a", self.a), ("b", self.b)):
            if not report:
                continue
            matches = [n for n in report.tests if n == name] or [n for n in report.tests if name in n]
            if not matches:
                continue
            found = True
            for n in matches[:3]:
                t = report.tests[n]
                print(f"[{tag}] {report.label} :: {n}")
                print(f"    status={t.status} error_type={t.error_type}")
                if t.output:
                    print(wrap(t.output))
                print()
        if not found:
            print(f"no test matching '{name}' found in selected report(s)")

    def do_categories(self, arg):
        "categories -- show how failure categories shifted between a and b (only tests present in both)."
        if not self._require(both=True):
            return
        shifted: dict[tuple[str, str], list[str]] = {}
        for n in self.diff.still_failing_changed_cat:
            key = (self.a.tests[n].category, self.b.tests[n].category)
            shifted.setdefault(key, []).append(n)
        if not shifted:
            print("no category changes among still-failing tests")
        for (ca, cb), names in shifted.items():
            print(f"  {ca} -> {cb}  ({len(names)})")
            for n in names:
                print(f"      {n}")

    def do_bucket(self, arg):
        """bucket add <name> <regex> | bucket rm <name> | bucket list
        Manage saved regex buckets (persisted across runs) for matching recurring
        failure output, e.g.:
            bucket add missing_handler "no handler for syscall_num: (\\d+)"
        Then use `list missing_handler` to see every test hitting that pattern."""
        try:
            tokens = shlex.split(arg)
        except ValueError as e:
            print(f"parse error: {e}")
            return
        if not tokens or tokens[0] not in ("add", "rm", "list"):
            print("usage: bucket add <name> <regex>\n       bucket rm <name>\n       bucket list")
            return
        sub = tokens[0]
        if sub == "list":
            if not self.custom_buckets:
                print("no custom buckets defined")
            for name, pattern in self.custom_buckets.items():
                print(f"  {name!r}: {pattern}")
            return
        if len(tokens) < 2:
            print(f"usage: bucket {sub} <name> ...")
            return
        name = tokens[1]
        if sub == "rm":
            if self.custom_buckets.pop(name, None) is None:
                print(f"no such bucket: {name}")
            else:
                save_buckets(self.custom_buckets)
                print(f"removed bucket {name!r}")
            return
        if len(tokens) < 3:
            print("usage: bucket add <name> <regex>")
            return
        # shlex already stripped one layer of quoting; rejoin in case the
        # regex itself contained un-quoted spaces.
        pattern = " ".join(tokens[2:])
        try:
            re.compile(pattern)
        except re.error as e:
            print(f"invalid regex: {e}")
            return
        self.custom_buckets[name] = pattern
        save_buckets(self.custom_buckets)
        print(f"saved bucket {name!r} = {pattern}  (in {BUCKETS_FILE})")

    def complete_bucket(self, text, line, begidx, endidx):
        args = line[:begidx].split()
        if len(args) <= 1:
            return self._complete_from(text, ["add", "rm", "list"])
        if len(args) == 2 and args[1] == "rm":
            return self._complete_from(text, list(self.custom_buckets))
        return []

    def do_quit(self, _arg):
        "Exit."
        return True

    do_exit = do_quit
    do_EOF = do_quit


def print_full_diff(a: Report, b: Report):
    d = Diff(a, b)
    print(d.summary())
    for bucket in ("newly_failing", "newly_fixed", "still_failing_changed_cat"):
        names = getattr(d, bucket)
        if not names:
            continue
        print(f"\n== {bucket} ({len(names)}) ==")
        for n in names:
            extra = ""
            if bucket == "still_failing_changed_cat":
                extra = f"  [{a.tests[n].category} -> {b.tests[n].category}]"
            sys.stdout.write(f"  {n}{extra}\n")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--reports-dir", default=str(REPORTS_DIR), help="directory containing report runs")
    ap.add_argument("--a", help="baseline report (index, folder name, or path)")
    ap.add_argument("--b", help="current report (index, folder name, or path)")
    ap.add_argument("--diff-only", action="store_true", help="print diff and exit instead of entering the REPL")
    args = ap.parse_args()

    reports_dir = Path(args.reports_dir)
    shell = ReportShell(reports_dir)

    if args.a:
        pa = find_report(args.a, shell.candidates)
        if not pa:
            sys.exit(f"could not resolve --a {args.a!r}")
        shell.a = Report.load(pa)
    if args.b:
        pb = find_report(args.b, shell.candidates)
        if not pb:
            sys.exit(f"could not resolve --b {args.b!r}")
        shell.b = Report.load(pb)
        shell._refresh_diff()

    if args.diff_only:
        if not (shell.a and shell.b):
            sys.exit("--diff-only requires both --a and --b")
        print_full_diff(shell.a, shell.b)
        return

    shell.cmdloop()


if __name__ == "__main__":
    main()
