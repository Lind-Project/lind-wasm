# Pipelines

We use two CI/CD platforms across three repositories. This section documents the
pipeline setup for each repository.

## Platforms

Pipelines run on **GitHub Actions (GHA)** and **Google Cloud Platform (GCP)**.
The two platforms have separate responsibilities and operate independently.

| Platform | Role |
| --- | --- |
| GitHub Actions | Code validation on every pull request — lint checks, end-to-end tests, and security scans |
| Google Cloud Platform | Docker image builds and publishing — runs resource-intensive builds that exceed GHA's disk and CPU limits |

GCP is used for builds that exceed GHA runner disk and CPU limits — primarily
Docker image builds that compile large codebases from source.



## Image Dependencies

The Lind project consists of three repositories with a clear dependency chain.
`lind-wasm` is the core runtime; the other two repositories build on top of it.

```
lind-wasm
   │
   │  produces
   ▼
securesystemslab/lind-wasm-dev
   │
   ├──────────────────────────────┐
   │                              │
   ▼                              ▼
lind-wasm-apps           lind-wasm-example-grates
```

`lind-wasm-apps` and `lind-wasm-example-grates` both pull the `lind-wasm-dev` image as their base.