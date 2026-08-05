# Releasing lind-wasm

This document describes how lind-wasm releases are versioned, where to find them,
and how a new release is created.

## 1. Releases

### 1.1 Current release

**v0.1.0**

| Property | Value |
| --- | --- |
| Release page | [v0.1.0](https://github.com/Lind-Project/lind-wasm/releases/tag/v0.1.0) |
| Target commit | `02cbc3e76` |
| Docker image | `securesystemslab/lind-wasm:v0.1.0` |
| Digest | `sha256:97ec75e99295c924971de249925ac7a3dd136ceb002b198a9e5b30e440ad7e28` |

This is a pre-release. lind-wasm has not reached `v1.0.0` yet.

### 1.2 Where to find releases

- **GitHub Releases** — [all releases](https://github.com/Lind-Project/lind-wasm/releases),
- **Docker Hub** — [securesystemslab/lind-wasm](https://hub.docker.com/r/securesystemslab/lind-wasm/tags)

Pull a version tag to get a fixed, reproducible image:

```bash
docker pull securesystemslab/lind-wasm:v0.1.0
```

The `latest` tag tracks the most recent commit on `main`, not the most recent release. Use a version tag if you need a stable image.



## 2. Docker Images

The release image is built and pushed to Docker Hub as `securesystemslab/lind-wasm` on every push to `main`. Version tags (`vX.Y.Z`) are applied on top of those builds when a release is created.

See [Pipelines — lind-wasm](https://lind-project.github.io/lind-wasm/contribute/pipelines/lind-wasm/)
for full pipeline details.

## 3. Versioning Policy

We use [Semantic Versioning](https://semver.org/) (`vMAJOR.MINOR.PATCH`).

lind-wasm is currently in `0.x.x`. This is because the project is still under development and the release process is still being set up.

Criteria for `v1.0.0` are still under discussion. This section will be updated once those criteria are finalized.

## 4. Release Plan

There is no fixed release schedule yet. Minor versions (`v0.x.0`) are created manually using the process below.

Once the criteria for `v1.0.0` are agreed on, official releases will begin from that point.

## 5. Tag Strategy

| Tag | Purpose | How it's created |
| --- | --- | --- |
| `sha-<hash>` | Traceable to a specific commit | Pushed automatically on every push to `main` |
| `vX.Y.Z` | Official release | Currently manual |

`vX.Y.Z` images are not rebuilt at release time. They are re-tagged from an existing `sha-<hash>` image that was already built and pushed from `main`. This guarantees the release image matches what was already tested.

## 6. How to Set a Release

### 6.1 Decide the version number

Pick a version number following SemVer.

### 6.2 Confirm the target commit

Check that the commit on `main` you want to release has a passing CI run.

```bash
git log -1 --oneline main
gh run list --branch main --limit 5
```

### 6.3 Confirm the image exists

`release.yml` pushes a `sha-<hash>` image on every push to `main`.

Confirm the image for the target commit already exists.

```bash
docker pull securesystemslab/lind-wasm:sha-<short-sha>
```

If it doesn't exist, trigger the workflow manually:

```bash
gh workflow run release.yml --ref main
```

### 6.4 Create the GitHub Release

Use `--target main` if the commit is the current `main` HEAD. Use a full 40-character SHA otherwise — short SHAs are not accepted by the GitHub Release API.

```bash
gh release create vX.Y.Z \
  --target main \
  --title "Lind vX.Y.Z" \
  --prerelease \
  --notes "..."
```

Use `--prerelease` while the version is below `v1.0.0`.

### 6.5 Re-tag and push the Docker image

Do not rebuild. Reuse the existing `sha-<hash>` image.

```bash
docker tag securesystemslab/lind-wasm:sha-<short-sha> securesystemslab/lind-wasm:vX.Y.Z
docker push securesystemslab/lind-wasm:vX.Y.Z
```

### 6.6 Verify the digest and update release notes

Confirm the `sha-<hash>` image and the `vX.Y.Z` image have the same digest.

```bash
docker inspect --format='{{index .RepoDigests 0}}' securesystemslab/lind-wasm:sha-<short-sha>
docker inspect --format='{{index .RepoDigests 0}}' securesystemslab/lind-wasm:vX.Y.Z
```

Add the digest to the GitHub Release notes:

```
Docker image: securesystemslab/lind-wasm:vX.Y.Z
Digest: sha256:<digest>
```