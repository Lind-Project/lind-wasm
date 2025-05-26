import json
import sys
from jinja2 import Environment, FileSystemLoader, select_autoescape

ALLOWED_CRATE_DIRS = [
    "src/RawPOSIX",
    "src/fdtables",
    "src/sysdefs",
    "src/wasmtime",
]

def filter_messages(messages, allowed_dirs):
    filtered = []
    for msg in messages:
        if msg.get("reason") != "compiler-message":
            continue
        src_path = msg.get("target", {}).get("src_path", "")
        if not any(d in src_path for d in allowed_dirs):
            continue

        spans = msg.get("message", {}).get("spans", [])
        if not spans:
            continue

        for span in spans:
            code_line = span.get("text", [{}])[0].get("text", "(No source line in span)")

            filtered.append({
                "file": span.get("file_name"),
                "line": span.get("line_start"),
                "level": msg["message"].get("level"),
                "message": msg["message"].get("message"),
                "code": code_line,
            })
    return filtered

def render_html(issues, output_path):
    env = Environment(
        loader=FileSystemLoader(searchpath="."),
        autoescape=select_autoescape(["html"])
    )

    template = env.from_string("""
    <!DOCTYPE html>
    <html>
    <head>
        <title>Clippy Report</title>
        <style>
            body { font-family: sans-serif; margin: 2em; }
            table { border-collapse: collapse; width: 100%; }
            th, td { border: 1px solid #ccc; padding: 0.5em; text-align: left; }
            th { background: #eee; }
            .warning { background-color: #fff8dc; }
            .error { background-color: #ffe0e0; }
            pre.code { background-color: #f7f7f7; margin: 0; padding: 0.25em; font-family: monospace; }
        </style>
    </head>
    <body>
        <h1>Clippy Lint Report</h1>
        <p><strong>Total Issues:</strong> {{ issues|length }}</p>
        <table>
            <tr><th>File</th><th>Line</th><th>Level</th><th>Code</th><th>Message</th></tr>
            {% for issue in issues %}
            <tr class="{{ issue.level }}">
                <td>{{ issue.file }}</td>
                <td>{{ issue.line }}</td>
                <td>{{ issue.level }}</td>
                <td><pre class="code">{{ issue.code }}</pre></td>
                <td><pre>{{ issue.message }}</pre></td>
            </tr>
            {% endfor %}
        </table>
    </body>
    </html>
    """)

    with open(output_path, "w") as f:
        f.write(template.render(issues=issues))

def main():
    if len(sys.argv) != 2:
        print("Usage: python clippy_report.py <clippy_all.json>")
        sys.exit(1)

    input_file = sys.argv[1]
    messages = json.load(open(input_file))
    filtered = filter_messages(messages, ALLOWED_CRATE_DIRS)
    render_html(filtered, "clippy_report.html")

if __name__ == "__main__":
    main()
