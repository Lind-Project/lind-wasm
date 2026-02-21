#!/usr/bin/env bash
# gen_wasm_exports_from_glibc_versions.sh
#
# Finds (or reads) glibc "Versions" files, then runs the Python parser on each
# file (one by one), printing:
#   <path-to-Versions>:
#     <symbol or flag>
#     <symbol or flag>
#
# Usage:
#   ./gen_wasm_exports_from_glibc_versions.sh <glibc-root> <parse_versions.py> \
#       [--include-private] [--flags] [--out FILE] [--paths-file FILE]
#
# Options:
#   --include-private   Include GLIBC_PRIVATE symbols
#   --flags             Output wasm-ld flags: --export-if-defined=<sym>
#   --out FILE          Write output to FILE (default: stdout)
#   --paths-file FILE   Read Versions paths from FILE instead of running find.
#                       One path per line. Relative paths are interpreted
#                       relative to <glibc-root>. Blank lines and lines
#                       starting with '#' are ignored.
#
# Notes:
# - Output is grouped per Versions file and is NOT globally deduped.
# - If you want a deduped global list, post-process separately.

set -euo pipefail

if [[ $# -lt 2 ]]; then
  echo "usage: $0 <glibc-root> <parse_versions.py> [--include-private] [--flags] [--out FILE] [--paths-file FILE]" >&2
  exit 2
fi

GLIBC_ROOT="$1"
PY_SCRIPT="$2"
shift 2

INCLUDE_PRIVATE=0
FLAGS=0
OUTFILE=""
PATHS_FILE=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --include-private) INCLUDE_PRIVATE=1; shift ;;
    --flags) FLAGS=1; shift ;;
    --out)
      OUTFILE="${2:-}"
      shift 2
      ;;
    --paths-file)
      PATHS_FILE="${2:-}"
      shift 2
      ;;
    -h|--help)
      sed -n '1,90p' "$0"
      exit 0
      ;;
    *)
      echo "error: unknown arg: $1" >&2
      exit 2
      ;;
  esac
done

if [[ ! -d "$GLIBC_ROOT" ]]; then
  echo "error: glibc root not found: $GLIBC_ROOT" >&2
  exit 2
fi

if [[ ! -f "$PY_SCRIPT" ]]; then
  echo "error: python script not found: $PY_SCRIPT" >&2
  exit 2
fi

if [[ -n "$PATHS_FILE" && ! -f "$PATHS_FILE" ]]; then
  echo "error: paths file not found: $PATHS_FILE" >&2
  exit 2
fi

if [[ -n "$OUTFILE" ]]; then
  exec > "$OUTFILE"
fi

args=()
if [[ "$INCLUDE_PRIVATE" -eq 1 ]]; then
  args+=(--include-private)
fi
if [[ "$FLAGS" -eq 1 ]]; then
  args+=(--flags)
fi

emit_one() {
  local vf="$1"
  #echo "${vf}:"
  python3 "$PY_SCRIPT" "${args[@]}" "$vf" | sed 's/^/    /'
  #echo
}

if [[ -n "$PATHS_FILE" ]]; then
  # Read one path per line; ignore blank lines and comments.
  # Relative paths are resolved against GLIBC_ROOT.
  while IFS= read -r line || [[ -n "$line" ]]; do
    # trim leading/trailing whitespace
    line="${line#"${line%%[![:space:]]*}"}"
    line="${line%"${line##*[![:space:]]}"}"

    [[ -z "$line" ]] && continue
    [[ "${line:0:1}" == "#" ]] && continue

    if [[ "$line" = /* ]]; then
      vf="$line"
    else
      vf="$GLIBC_ROOT/$line"
    fi

    if [[ ! -f "$vf" ]]; then
      echo "warning: not found: $vf" >&2
      continue
    fi

    emit_one "$vf"
  done < "$PATHS_FILE"
else
  # Find all Versions files under the specified glibc root, stable order.
  find "$GLIBC_ROOT" -name "Versions" -type f -print0 | sort -z | \
  while IFS= read -r -d '' vf; do
    emit_one "$vf"
  done
fi
