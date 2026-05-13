# lind-wasm-apps

Its pipeline covers scheduled Docker image builds that cross-compile applications to run on the Lind runtime.

## Overview

This repository uses GCP for the Docker image build that publishes the applications image to Docker Hub.
GHA is not currently configured for this repository, but workflows will be added if validation pipelines are needed in the future.

| Platform | Role |
| --- | --- |
| GitHub Actions | Not currently configured |
| Google Cloud Platform | Builds and publishes `securesystemslab/lind-wasm-apps` to Docker Hub on a scheduled basis |

## GCP Cloud Build

Build configuration files are located under `scripts/` in the repository.

### Workflows

- **`app-build`** — Cloning the `main` branch at the time of execution. Builds the repository `Dockerfile` on top of `securesystemslab/lind-wasm-dev` and publishes the resulting image to Docker Hub as `securesystemslab/lind-wasm-apps`.

## Docker Images

The `lind-wasm-apps` pipeline publishes one Docker image to Docker Hub.

### `securesystemslab/lind-wasm-apps`

The runtime image containing the Lind environment and cross-compiled WASM application binaries, ready to execute immediately. Built on top of `securesystemslab/lind-wasm-dev`.

| Property | Detail |
| --- | --- |
| Source | `Dockerfile` in the repository root |
| Base image | `securesystemslab/lind-wasm-dev:latest` |
| Published by | GCP `app-build` |
| Tags | `latest` — most recent build; `sha-<commit>` — immutable snapshot for rollback |

The image currently includes the following applications: bash, coreutils, cpython, lmbench, sed, nginx, grep, curl, git, and postgres. Additional applications will be added in the future.

### Pulling the Images

```bash
# Latest applications image
docker pull securesystemslab/lind-wasm-apps:latest
```
