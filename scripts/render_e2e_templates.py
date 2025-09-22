#!/usr/bin/env python3
import os
from jinja2 import Environment, FileSystemLoader

def main():
    tpl_dir = "scripts/templates"
    env = Environment(loader=FileSystemLoader(tpl_dir))
    tpl = env.get_template("e2e_comment.md.j2")
    rendered = tpl.render()

    os.makedirs("test-reports", exist_ok=True)
    out_path = "test-reports/e2e_comment.md"
    with open(out_path, "w", encoding="utf-8") as f:
        f.write(rendered)

    print(f"Rendered {out_path}")

if __name__ == "__main__":
    main()
