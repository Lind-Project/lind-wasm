#!/usr/bin/env bash
# scripts/clippy_report_html.sh
set -euo pipefail

MANIFEST_PATH="src/wasmtime/Cargo.toml"
OUT_HTML="clippy_report.html"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --manifest-path) MANIFEST_PATH="${2:-}"; shift 2 ;;
    --out) OUT_HTML="${2:-}"; shift 2 ;;
    *) echo "Unknown arg: $1" >&2; exit 2 ;;
  esac
done

RAW_JSON="$(mktemp)"
JQ_FILTER="$(mktemp)"
trap 'rm -f "$RAW_JSON" "$JQ_FILTER"' EXIT

export CARGO_TERM_COLOR=never

if ! cargo clippy \
  --manifest-path "$MANIFEST_PATH" \
  --all-features \
  --keep-going \
  --message-format=json \
  -- \
  -A clippy::not_unsafe_ptr_arg_deref \
  -A clippy::absurd_extreme_comparisons \
  >"$RAW_JSON" 2>&1
then
  CLIPPY_STATUS=$?
else
  CLIPPY_STATUS=0
fi

cat >"$JQ_FILTER" <<'JQ'
def h:
  tostring
  | gsub("&"; "&amp;")
  | gsub("<"; "&lt;")
  | gsub(">"; "&gt;")
  | gsub("\""; "&quot;")
  | gsub("'"; "&#39;");

# Arg-less filter version; works with `... | nl_to_br`
def nl_to_br:
  gsub("\r?\n"; "<br>")
  | gsub("\\\\r?\\\\n"; "<br>");

def diag_to_html:
  (
    "<li class=\"" + .level + "\">"
    + "<details class=\"diag\" open><summary>"
    + "<code class=\"badge\">" + (.code|h) + "</code> — " + (.message|h)
    + ( if .span != null then
          " <span class=\"loc\">("
          + (.span.file_name|h) + ":"
          + (.span.line_start|tostring) + ":"
          + (.span.column_start|tostring) + "–"
          + (.span.line_end|tostring) + ":"
          + (.span.column_end|tostring) + ")</span>"
        else "" end )
    + "</summary>"
    + ( if (.rendered // "") != "" then
          "<div class=\"rendered\">" + ( (.rendered|h) | nl_to_br ) + "</div>"
        else "" end )
    + "</details></li>"
  );


[ inputs
  | fromjson?
  | select(.reason == "compiler-message")
  | select(.message.level == "warning" or .message.level == "error")
  | {
      crate: .target.name,
      file: (
              (.message.spans[]? | select(.is_primary == true) | .file_name)
              // (.message.spans[0]?.file_name // "unknown")
            ),
      code:     (.message.code?.code // "unknown"),
      level:    .message.level,
      message:  .message.message,
      rendered: (.message.rendered // ""),
      span:     (
                  (.message.spans[]? | select(.is_primary == true))
                  // .message.spans[0]
                )
    }
] as $diags
| ($diags | length) as $total
| ($diags | map(select(.level=="error"))   | length) as $errors
| ($diags | map(select(.level=="warning")) | length) as $warnings
| ($diags | sort_by(.crate, .file, .level, .code, .message)
          | group_by(.crate)) as $by_crate
| "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\">"
  + "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">"
  + "<title>Clippy Report</title>"
  + "<style>"
  + "body{font-family:system-ui,-apple-system,Segoe UI,Roboto,Ubuntu,Arial,sans-serif;margin:24px;line-height:1.45}"
  + "header{margin-bottom:18px}"
  + "h1{margin:0 0 6px 0}"
  + ".summary{font-weight:600;color:#333}"
  + "section.crate{margin:16px 0;padding:12px 16px;border:1px solid #ddd;border-radius:12px;background:#fafafa}"
  + "details.file{margin:10px 0}"
  + "details>summary{cursor:pointer}"
  + "ul.diagnostics{margin:8px 0 0 16px;padding:0;list-style:none}"
  + "ul.diagnostics li{margin:8px 0;padding:8px 10px;border-left:4px solid #ccc;background:#fff;border-radius:8px}"
  + "li.error{border-left-color:#c62828}"
  + "li.warning{border-left-color:#f9a825}"
  + "code.badge{display:inline-block;padding:2px 6px;border-radius:6px;background:#f0f0f0}"
  + ".path{color:#333}"
  + ".loc{opacity:.8;font-size:.9em}"
  + "details.diag summary{font-weight:600}"
  + ".rendered{background:#0b0c0e;color:#e6e6e6;padding:10px;border-radius:8px;overflow:auto;"
  + "font-family:ui-monospace,SFMono-Regular,Menlo,Monaco,Consolas,'Liberation Mono','Courier New',monospace;font-size:.9em;"
  + "white-space:pre-wrap}"  # preserve spacing for carets, we insert <br> for line breaks
  + "</style></head><body>"
  + "<header><h1>Clippy Report</h1>"
  + "<p class=\"summary\">Total: " + ($total|tostring)
  + " — Errors: " + ($errors|tostring)
  + " — Warnings: " + ($warnings|tostring) + "</p></header>"
  + (
      $by_crate
      | map(
          . as $c
          | ($c[0].crate) as $crate
          | ($c | group_by(.file)) as $files
          | "<section class=\"crate\"><h2>" + ($crate|h)
            + " (" + (($c|length)|tostring) + ")</h2>"
            + (
                $files
                | map(
                    . as $f
                    | ($f[0].file) as $file
                    | ($f | map({code, level, message, rendered, span})) as $ds
                    | ($ds|length) as $cnt
                    | ($ds | map(select(.level=="error"))   | length) as $errs
                    | ($ds | map(select(.level=="warning")) | length) as $warns
                    | "<details class=\"file\"><summary><span class=\"path\">"
                      + ($file|h) + "</span> — " + ($cnt|tostring)
                      + " issues (" + ($errs|tostring) + " errors, "
                      + ($warns|tostring) + " warnings)</summary>"
                      + "<ul class=\"diagnostics\">"
                      + ( $ds | map( diag_to_html ) | join("") )
                      + "</ul></details>"
                  )
                | join("")
              )
            + "</section>"
        )
      | join("")
    )
  + "</body></html>"
JQ

sed -r 's/\x1b\[[0-9;]*m//g' "$RAW_JSON" \
  | grep -E '^[[:space:]]*\{' \
  | jq -Rnc -f "$JQ_FILTER" > "$OUT_HTML"

echo "Wrote: $OUT_HTML"
exit "${CLIPPY_STATUS-0}"
