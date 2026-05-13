# lind-wasm-example-grates

Its pipeline covers scheduled Docker image builds that compile syscall interceptors (grates) to run on the Lind runtime.

## Overview

This repository uses GCP for the Docker image build that publishes the grates image to Docker Hub. GHA is not currently configured for this repository, but workflows will be added if validation pipelines are needed in the future.

| Platform | Role |
| --- | --- |
| GitHub Actions | Not currently configured |
| Google Cloud Platform | Builds and publishes `securesystemslab/lind-wasm-grates-examples` to Docker Hub on a scheduled basis |

## GCP Cloud Build

Build configuration files are located under `scripts/` in the repository.

### Workflows

- **`grates-build`** — Cloning the `main` branch at the time of execution. Builds the repository `Dockerfile` on top of `securesystemslab/lind-wasm-dev`, runs the grates test suite, and publishes the resulting image to Docker Hub as `securesystemslab/lind-wasm-grates-examples`. The image is pushed even when tests fail, so developers can inspect the container state to diagnose failures.

## Docker Images

The `lind-wasm-example-grates` pipeline publishes one Docker image to Docker Hub.

### `securesystemslab/lind-wasm-grates-examples`

The dev image containing the Lind runtime and compiled grates, primarily used for debugging test failures in-container. Built on top of `securesystemslab/lind-wasm-dev`.

| Property | Detail |
| --- | --- |
| Source | `Dockerfile` in the repository root |
| Base image | `securesystemslab/lind-wasm-dev:latest` |
| Published by | GCP `grates-build` |
| Tags | `latest` — most recent build; `sha-<commit>` — immutable snapshot for rollback |

### Pulling the Images

```bash
# Latest grates image
docker pull securesystemslab/lind-wasm-grates-examples:latest
```