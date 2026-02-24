#!/usr/bin/env python3
"""
Extract exported symbol names from glibc "Versions" files.

- Parses blocks like:
    libc {
      GLIBC_2.0 { printf; fprintf; }
      GLIBC_PRIVATE { _itoa_lower_digits; }
    }

- Outputs a deduped list of symbols, optionally excluding GLIBC_PRIVATE.
- Designed to be simple + robust for the common glibc Versions syntax.

Usage:
  python3 parse_versions.py path/to/Versions [more Versions files...]

Examples:
  # Print symbols (exclude GLIBC_PRIVATE by default)
  python3 parse_versions.py stdio-common/Versions

  # Include GLIBC_PRIVATE symbols too
  python3 parse_versions.py --include-private stdio-common/Versions

  # Emit wasm-ld flags
  python3 parse_versions.py --flags stdio-common/Versions

  # Write flags to a response file (handy to avoid command length limits)
  python3 parse_versions.py --flags --out exports.rsp $(git ls-files '*Versions')
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path
from typing import Iterable, Iterator, List, Set, Tuple


# Match a version node name like GLIBC_2.0 or GLIBC_PRIVATE
_RE_NODE_OPEN = re.compile(r"\b(GLIBC(?:_PRIVATE)?_\d+(?:\.\d+)*|GLIBC_PRIVATE)\b\s*\{")
_RE_BRACE = re.compile(r"[{}]")
_RE_COMMENT = re.compile(r"#.*?$", re.M)

# C identifier-ish symbols (glibc symbol names typically match this)
_RE_SYMBOL = re.compile(r"\b[A-Za-z_][A-Za-z0-9_]*\b")


def _strip_comments(text: str) -> str:
    return re.sub(_RE_COMMENT, "", text)


def _iter_version_nodes(text: str) -> Iterator[Tuple[str, str]]:
    """
    Yield (node_name, node_body_text) for each GLIBC_* { ... } node.
    Uses a simple brace-matching scan starting at the '{' after the node name.
    """
    i = 0
    n = len(text)
    while True:
        m = _RE_NODE_OPEN.search(text, i)
        if not m:
            return
        node = m.group(1)
        # position at the '{' that starts this node
        brace_open_idx = text.find("{", m.end() - 1)
        if brace_open_idx < 0:
            i = m.end()
            continue

        depth = 0
        j = brace_open_idx
        # scan forward to find matching closing brace for this node
        while j < n:
            c = text[j]
            if c == "{":
                depth += 1
            elif c == "}":
                depth -= 1
                if depth == 0:
                    body = text[brace_open_idx + 1 : j]
                    yield node, body
                    i = j + 1
                    break
            j += 1
        else:
            # Unbalanced braces; stop
            return


def _extract_symbols_from_body(body: str) -> Set[str]:
    """
    Extract symbols from within a node body. We only consider tokens that
    appear in "symbol;" statements.

    Heuristic:
      - split by ';' and extract identifier tokens from each statement.
      - ignore empty statements.
      - take the last identifier in each statement as the symbol name.
        (works well for 'foo;' and for most glibc patterns)
    """
    syms: Set[str] = set()
    for stmt in body.split(";"):
        stmt = stmt.strip()
        if not stmt:
            continue
        # Ignore linker-script directives/keywords that sometimes appear
        # (rare in glibc Versions, but harmless).
        # We'll just pick the last identifier token as the symbol.
        ids = _RE_SYMBOL.findall(stmt)
        if not ids:
            continue
        sym = ids[-1]
        # Filter out obvious non-symbol keywords if they show up
        if sym in {"global", "local"}:
            continue
        syms.add(sym)
    return syms


def extract_symbols_from_versions_file(
    path: Path, include_private: bool = False
) -> Set[str]:
    text = path.read_text(encoding="utf-8", errors="replace")
    text = _strip_comments(text)

    out: Set[str] = set()
    for node, body in _iter_version_nodes(text):
        if (not include_private) and node == "GLIBC_PRIVATE":
            continue
        out |= _extract_symbols_from_body(body)
    return out


def iter_paths(inputs: Iterable[str]) -> List[Path]:
    paths: List[Path] = []
    for s in inputs:
        p = Path(s)
        if p.is_dir():
            # Collect all files named 'Versions' under this directory
            paths.extend(sorted(p.rglob("Versions")))
        else:
            paths.append(p)
    return paths


def main(argv: List[str]) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument(
        "inputs",
        nargs="+",
        help="Versions files or directories to scan (directories will be searched recursively for files named 'Versions')",
    )
    ap.add_argument(
        "--include-private",
        action="store_true",
        help="Include symbols from GLIBC_PRIVATE blocks (default: exclude)",
    )
    ap.add_argument(
        "--flags",
        action="store_true",
        help="Output one wasm-ld flag per line: --export-if-defined=<sym>",
    )
    ap.add_argument(
        "--out",
        type=str,
        default="",
        help="Write output to this file instead of stdout",
    )
    ap.add_argument(
        "--sort",
        action="store_true",
        help="Sort output alphabetically (default: sorted)",
    )
    args = ap.parse_args(argv)

    paths = iter_paths(args.inputs)
    missing = [p for p in paths if not p.exists()]
    if missing:
        for p in missing:
            print(f"error: not found: {p}", file=sys.stderr)
        return 2

    all_syms: Set[str] = set()
    for p in paths:
        if p.is_file():
            all_syms |= extract_symbols_from_versions_file(
                p, include_private=args.include_private
            )

    # default to sorted for stable output
    syms = sorted(all_syms) if (args.sort or True) else list(all_syms)

    lines: List[str]
    if args.flags:
        lines = [f"--export-if-defined={s} \\" for s in syms]
    else:
        lines = syms

    output = "\n".join(lines) + "\n"

    if args.out:
        Path(args.out).write_text(output, encoding="utf-8")
    else:
        sys.stdout.write(output)

    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
