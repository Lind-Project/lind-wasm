#!/usr/bin/env python3
import os, re, pathlib
from jinja2 import Environment, FileSystemLoader, select_autoescape

def read(path):
    return pathlib.Path(path).read_text(encoding="utf-8", errors="replace")

def extract_body(html_text:str) -> str:
    m = re.search(r"(?is)<\s*body\b>[^>]*>(.*?)</\s*body\s*>",html_text, flags=re.I)
    return m.group(1) if m else html_text

def wrap_h3_tables_as_details(html: str) -> str:    
    import re
    pattern = re.compile(r'(\s*<h3[^>]*>.*?</h3>\s*)(<table\b.*?>.*?</table>)',
                         re.DOTALL | re.IGNORECASE)
    def _wrap(m):
        h3 = m.group(1).strip()
        table = m.group(2)
        return f'  <summary>{h3}</summary>\n <details>\n {table}\n</details>'
    return pattern.sub(_wrap, html)

def main():
    report_path = os.environ["REPORT_PATH"]
    
    tpl_dir = "scripts/templates"
    out_dir = "test-reports"
    os.makedirs(out_dir,exist_ok=True)
    
    body_html = extract_body(read(report_path))
    body_html = wrap_h3_tables_as_details(body_html)

    env = Environment(
        loader=FileSystemLoader(tpl_dir),
        autoescape=select_autoescape(["md","j2","html"]),
        trim_blocks=True,
        lstrip_blocks=True
    )
   

    md = env.get_template("e2e_comment.md.j2").render(html_body=body_html)
    pathlib.Path(f"{out_dir}/e2e_comment.md").write_text(md, encoding="utf-8")

    os.makedirs("test-reports", exist_ok=True)
    out_path = "test-reports/e2e_comment.md"
    with open(out_path, "w", encoding="utf-8") as f:
        f.write(md)

    print(f"Rendered {out_path}")

if __name__ == "__main__":
    main()
