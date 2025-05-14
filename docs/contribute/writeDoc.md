# How to Add Documentation

Built with *mkdocs* and *material*, and hosted on *GitHub pages*:
[lind-project.github.io/lind-wasm](https://lind-project.github.io/lind-wasm).

You can improve the docs by editing the files below. See [`mkdocs`](https://www.mkdocs.org/)
and [`material`](https://squidfunk.github.io/mkdocs-material/) user guides for
details. And don't forget to try out your changes locally!

## Important files
- [`.github/workflows/docs.yml`](https://github.com/Lind-Project/lind-wasm/blob/main/.github/workflows/docs.yml): Auto-deploys on push to `main` (e.g. on PR merge)
- [`mkdocs.yml`](https://github.com/Lind-Project/lind-wasm/blob/main/mkdocs.yml): Site config (e.g. navigation and plugins)
- [`docs/`](https://github.com/Lind-Project/lind-wasm/tree/main/docs): Site sources

## Build site locally
```bash
# Install requirements (hint: use a virtual environment)
pip install mkdocs-material

# Run dev server and check output!
mkdocs serve
```
