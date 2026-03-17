#!/usr/bin/env python3
import os, re, pathlib
from jinja2 import Environment, FileSystemLoader, select_autoescape

def read(path):
    return pathlib.Path(path).read_text(encoding="utf-8", errors="replace")

def extract_body(html_text:str) -> str:
    m = re.search(r"(?is)<\s*body\b>[^>]*>(.*?)</\s*body\s*>",html_text, flags=re.I)
    return m.group(1) if m else html_text

def strip_top_h1(html: str) -> str:
    # Remove the first <h1>…</h1> so it doesn't render as a blob in the PR comment.
    return re.sub(r'(?is)<\s*h1\b[^>]*>.*?</\s*h1\s*>', '', html, count=1)

def wrap_sections_as_details(html: str) -> str:
    
    #Wrap each <div class="test-section">…</div> as:
    #  <details><summary>{h2 text}</summary>…(section content without the h2)…</details>
    
    pattern = re.compile(
        r"""
        <div
            [^>]*
            class="[^"]*\btest-section\b[^"]*"
            [^>]*>
            \s*
            (?:<h2[^>]*>(?P<title>.*?)</h2>)?
            (?P<body>.*?)
        </div>
        """,
        re.IGNORECASE | re.DOTALL | re.VERBOSE,
    )

    def _wrap(m):
        title = (m.group("title") or "Section").strip()
        body = m.group("body")
        return f"<details>\n  <summary>{title}</summary>\n{body}\n</details>"

    return pattern.sub(_wrap, html)

def wrap_h3_tables_as_details(html: str):    
    import re
    pattern = re.compile(r'(\s*<h3[^>]*>.*?</h3>\s*)(<table\b.*?>.*?</table>)',
                         re.DOTALL | re.IGNORECASE)
    def _wrap(m):
        h3 = m.group(1).strip()
        table = m.group(2)
        return f' <summary>{h3}</summary>\n <details>\n {table}\n</details>'
    return pattern.sub(_wrap, html)

def main():
    report_path = os.environ["REPORT_PATH"]
    
    tpl_dir = "scripts/templates"
    out_dir = os.environ.get("OUT_DIR", ".")
    os.makedirs(out_dir, exist_ok=True)
    
    body_html = extract_body(read(report_path))
    body_html = strip_top_h1(body_html)
    
    _wrapped = wrap_sections_as_details(body_html)
    body_html = _wrapped if _wrapped != body_html else wrap_h3_tables_as_details(body_html)

    env = Environment(
        loader=FileSystemLoader(tpl_dir),
        autoescape=select_autoescape(["md","j2","html"]),
        trim_blocks=True,
        lstrip_blocks=True
    )
   
    md = env.get_template("e2e_comment.md.j2").render(html_body=body_html)
    out_path = pathlib.Path(out_dir) / "e2e_comment.md"
    out_path.write_text(md, encoding="utf-8")

    print(f"Rendered {out_path}")

if __name__ == "__main__":
    main()
