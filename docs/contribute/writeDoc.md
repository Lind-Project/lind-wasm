# How to Add Documentation

This doc is using the `material` for `mkdocs`, which turns markdown files into an online doc website.

**You should have `mkdocs` installed on your terminal first.**

**Do not edit on `gh-pages` branch, this is updated by github action automatically.**

To add a new seperate page of documentation, you need to

1. add a new `.md` markdown file to the `docs/` directory
2. in the `mkdocs.yml` file's `nav:` section, add a the new page's path
3. **IMPORTANT**: test if the changes works as expected by running `mkdocs serve` in the root directory of this repo, only push the changes after checking it works

NOTE: a github action of compiling these markdown files has already been setup. Once the changes of `.md` files are pushed, the static website branch will be automatically updated shortly.