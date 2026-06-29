# Pipelines

We use two CI/CD platforms across three repositories. This section documents the
pipeline setup for each repository.

## Platforms

Pipelines run on **GitHub Actions (GHA)** and **Google Cloud Platform (GCP)**.

GitHub Actions handles code validation on every pull request — lint checks,
end-to-end tests, and security scans.

GCP is used for large-scale builds and tests that exceed GHA's resource
limits.



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