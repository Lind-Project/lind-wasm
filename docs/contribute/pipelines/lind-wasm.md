# lind-wasm

Its pipeline covers both code validation on pull requests and Docker image builds.

## Overview

This repository uses both GHA and GCP. GHA validates code on every pull request,
while GCP handles the scheduled Docker image build that publishes the development image to Docker Hub.

| Platform | Role |
| --- | --- |
| GitHub Actions | Runs lint, end-to-end tests, and security scans on pull requests targeting `main`; publishes the release image |
| Google Cloud Platform | Builds and publishes `securesystemslab/lind-wasm-dev` to Docker Hub on a daily schedule |

## GitHub Actions

Workflow files are located under `.github/workflows/` in the repository.

### Workflows

- **`lint.yml`** — Runs `cargo fmt --check` and `cargo clippy` for code formatting
and static analysis.

- **`e2e.yml`** — Builds and runs the full test suite inside a container. Uploads
HTML and JSON test reports as artifacts and posts a summary comment to the PR. 
  
    - [Testing](../testing.md) for details on test.
    - [End-to-End Testing](../e2e-testing.md) for details on e2e structure.

- **`zizmor.yml`** — Scans GHA workflow files for security vulnerabilities such as
script injection and overly broad permissions.

- **`docs.yml`** — Builds and deploys the project documentation site to GitHub Pages.

- **`pr-cache-cleanup.yml`** — Removes the GHA build cache associated with a closed
PR to keep storage usage under control.

- **`release.yml`** — Builds and pushes the `release` stage as `securesystemslab/lind-wasm` to Docker Hub.

### Workflow Triggers

| Event | Workflows Triggered |
| --- | --- |
| PR opened or updated (non-draft, targeting `main`) | `lint.yml`, `e2e.yml`, `zizmor.yml` |
| Push to `main` | `lint.yml`, `docs.yml`, `release.yml` |
| PR closed | `pr-cache-cleanup.yml` |


## GCP Cloud Build

Build configuration files are located under `scripts/` in the repository.

### Workflows

**`dev-build`** — Cloning the `main` branch at the time of execution. Builds `Docker/Dockerfile.dev` and publishes the resulting image to Docker Hub as `securesystemslab/lind-wasm-dev`.

## Docker Images

The `lind-wasm` pipeline publishes two Docker images to Docker Hub.

### `securesystemslab/lind-wasm-dev`

The development image containing the full Lind toolchain for building and running WASM applications. Used as the base image for `lind-wasm-apps` and `lind-wasm-example-grates`.

| Property | Detail |
| --- | --- |
| Source | `Docker/Dockerfile.dev` |
| Published by | GCP `dev-build` (daily) |
| Tags | `latest` — most recent build; `sha-<commit>` — immutable snapshot for rollback |
| Update frequency | Daily at 08:00 America/New_York |

### `securesystemslab/lind-wasm`

| Property | Detail |
| --- | --- |
| Source | `Docker/Dockerfile.e2e` (`release` stage) |
| Published by | GHA `release.yml` (on push to `main`) |
| Tags | `latest` — most recent build; `sha-<commit>` — immutable snapshot for rollback |
| Version tags | `vX.Y.Z` — manually applied after a corresponding GitHub Release is created (e.g. `v0.1.0`) |
| Update frequency | On every push to `main` |

### Pulling the Images

```bash
# Latest development image
docker pull securesystemslab/lind-wasm-dev:latest

# Latest release image
docker pull securesystemslab/lind-wasm:latest
```