#!/usr/bin/env bash
set -Eeuo pipefail

# Dedicated dlopen positive-path smoke test for CI.
# Emits reports/dlopen.json with number_of_failures=0|1 and returns matching exit code.

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"

REPORTS_DIR="${REPO_ROOT}/reports"
REPORT_FILE="${REPORTS_DIR}/dlopen.json"

LIB_SOURCE="tests/unit-tests/dylink_tests/deterministic/lib.c"
MAIN_SOURCE="tests/unit-tests/dylink_tests/deterministic/main.c"
EXPECTED_SUBSTRING="Hello, main module! (from shared library)"

mkdir -p "${REPORTS_DIR}"
cd "${REPO_ROOT}"

tmp_dir="$(mktemp -d)"
manifest_file="${tmp_dir}/checks.tsv"
cleanup() {
  rm -rf "${tmp_dir}"
}
trap cleanup EXIT

pass_count=0
fail_count=0
skip_count=0

compile_lib_output="${tmp_dir}/compile_library.log"
compile_main_output="${tmp_dir}/compile_main.log"
run_main_output="${tmp_dir}/run_main.log"
verify_output_log="${tmp_dir}/verify_output_substring.log"
run_main_status="skipped"

compile_lib_cmd="${REPO_ROOT}/scripts/lind_compile --compile-library ${LIB_SOURCE}"
if "${REPO_ROOT}/scripts/lind_compile" --compile-library "${LIB_SOURCE}" > "${compile_lib_output}" 2>&1; then
  compile_lib_status="pass"
  pass_count=$((pass_count + 1))
else
  compile_lib_status="fail"
  fail_count=$((fail_count + 1))
fi
printf '%s\t%s\t%s\t%s\n' "compile_library" "${compile_lib_status}" "${compile_lib_cmd}" "${compile_lib_output}" >> "${manifest_file}"

compile_main_cmd="${REPO_ROOT}/scripts/lind_compile ${MAIN_SOURCE}"
if "${REPO_ROOT}/scripts/lind_compile" "${MAIN_SOURCE}" > "${compile_main_output}" 2>&1; then
  compile_main_status="pass"
  pass_count=$((pass_count + 1))
else
  compile_main_status="fail"
  fail_count=$((fail_count + 1))
fi
printf '%s\t%s\t%s\t%s\n' "compile_main" "${compile_main_status}" "${compile_main_cmd}" "${compile_main_output}" >> "${manifest_file}"

run_main_cmd="${REPO_ROOT}/scripts/lind_run main.cwasm"
if [[ "${compile_main_status}" = "pass" ]]; then
  if "${REPO_ROOT}/scripts/lind_run" main.cwasm > "${run_main_output}" 2>&1; then
    run_main_status="pass"
    pass_count=$((pass_count + 1))
  else
    run_main_status="fail"
    fail_count=$((fail_count + 1))
  fi
else
  run_main_status="skipped"
  skip_count=$((skip_count + 1))
  printf '%s\n' "Skipped: run step requires compile_main pass." > "${run_main_output}"
fi
printf '%s\t%s\t%s\t%s\n' "run_main" "${run_main_status}" "${run_main_cmd}" "${run_main_output}" >> "${manifest_file}"

verify_cmd="grep -Fq '${EXPECTED_SUBSTRING}' run_main_output"
if [[ "${run_main_status}" = "pass" ]]; then
  if grep -Fq "${EXPECTED_SUBSTRING}" "${run_main_output}"; then
    verify_status="pass"
    pass_count=$((pass_count + 1))
    printf '%s\n' "Matched expected substring: ${EXPECTED_SUBSTRING}" > "${verify_output_log}"
  else
    verify_status="fail"
    fail_count=$((fail_count + 1))
    {
      printf '%s\n' "Expected substring not found: ${EXPECTED_SUBSTRING}"
      printf '\n'
      printf '%s\n' "Observed run output:"
      cat "${run_main_output}"
    } > "${verify_output_log}"
  fi
else
  verify_status="skipped"
  skip_count=$((skip_count + 1))
  printf '%s\n' "Skipped: output verification requires run_main pass." > "${verify_output_log}"
fi
printf '%s\t%s\t%s\t%s\n' "verify_output_substring" "${verify_status}" "${verify_cmd}" "${verify_output_log}" >> "${manifest_file}"

python3 - "${manifest_file}" "${REPORT_FILE}" "${pass_count}" "${fail_count}" "${skip_count}" <<'PY'
import json
import pathlib
import sys

manifest_path, report_path, pass_count, fail_count, skip_count = sys.argv[1:]
checks = []
for raw in pathlib.Path(manifest_path).read_text(encoding="utf-8", errors="replace").splitlines():
    if not raw.strip():
        continue
    name, status, command, output_path = raw.split("\t", 3)
    output = pathlib.Path(output_path).read_text(encoding="utf-8", errors="replace")
    checks.append({
        "name": name,
        "status": status,
        "command": command,
        "output": output,
    })

summary = {
    "total": len(checks),
    "passed": int(pass_count),
    "failed": int(fail_count),
    "skipped": int(skip_count),
}

report = {
    "number_of_failures": summary["failed"],
    "status": "pass" if summary["failed"] == 0 else "fail",
    "test": "dlopen_smoke",
    "checks": checks,
    "summary": summary,
    "results": [],
}

if summary["failed"] > 0:
    report["error"] = "dlopen smoke failed"

pathlib.Path(report_path).write_text(json.dumps(report, ensure_ascii=True), encoding="utf-8")
PY

if [[ "${fail_count}" -eq 0 ]]; then
  printf '%s\n' "dlopen smoke test: PASS (passed=${pass_count}, failed=${fail_count}, skipped=${skip_count})"
  exit 0
fi

printf '%s\n' "dlopen smoke test: FAIL (passed=${pass_count}, failed=${fail_count}, skipped=${skip_count})" >&2
awk -F'\t' '$2=="fail" {print "failed_check: " $1}' "${manifest_file}" >&2
exit 1
