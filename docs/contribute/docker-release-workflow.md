# Docker Hub Release Workflow


The workflow builds the **lind-wasm** Docker image from the *release* stage of `Dockerfile.e2e` and pushes it to Docker Hub as **`securesystemslab/lind-wasm`**.  
It is **manual-only** (`workflow_dispatch`), so contributors must trigger it on demand.

---

## 1. Prerequisites

| Requirement | Purpose |
|-------------|---------|
| Write rights on this repo | lets you add secrets & trigger the workflow |
| Docker Hub access-token (or password) for `securesystemslab` | used by GitHub Actions to authenticate the Docker Hub image push |


> **Use a token, not your password:**  
> In Docker Hub ▸ **Account Settings → Security**, create a *Read/Write* access token and rotate it regularly.

---

## 2. Add the Docker Hub secret

1. GitHub repo ▸ **Settings → Secrets and variables → Actions**  
2. **New repository secret**  
   * **Name:** `DOCKERHUB_PASSWORD`  
   * **Value:** *your access token*  
3. Click **Add secret**

The workflow references it as `secrets.DOCKERHUB_PASSWORD`.  
See GitHub’s guide: **[Using secrets in GitHub Actions](https://docs.github.com/en/actions/security-guides/using-secrets-in-github-actions)**.

---

## 3. Run the workflow (manual trigger)

1. Open the repo’s **Actions** tab.  
2. Select **Build & push lind-wasm image**.  
3. Click **Run workflow**, choose a branch (defaults to `main`), then **Run**.  
4. Watch the logs: you should see **build → login → push** succeed.

For details, check GitHub’s **[Manually running a workflow](https://docs.github.com/en/actions/using-workflows/manually-running-a-workflow)** guide.

---

## 4. What the workflow does

1. **Checkout** the repository code.  
2. **Build** the Docker image from the *release* stage of `Dockerfile.e2e`.  
3. **Login** to Docker Hub with `DOCKERHUB_PASSWORD`.  
4. **Tag & push**:  
   * `securesystemslab/lind-wasm:<GIT_SHA>` – every build  
   * `securesystemslab/lind-wasm:latest` – every build

