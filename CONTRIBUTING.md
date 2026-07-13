# Contributing to Lind

Thanks for your interest in contributing to Lind! Lind is a community-driven,
open-source project, and it grows through the people who use it, file issues,
improve the docs, and send patches. Contributions of every size are welcome.

This guide explains how to get started, what you can work on, and how we review
and merge changes. For deeper, topic-specific guidance, see the
[contributor documentation](https://lind-project.github.io/lind-wasm/contribute/).

## Our Vision

Lind is a framework for safely executing untrusted POSIX applications inside a
single, unprivileged host process, without kernel modifications or elevated
privileges. Its design separates the **isolation mechanism** (how an
application's memory and control flow are confined) from the **mediation and
policy layer** (how its system calls are intercepted, routed, and serviced).
WebAssembly is the mature isolation backend today, and the project is
generalizing toward a pluggable, multi-backend framework. We believe strong
isolation with preserved POSIX compatibility should be open, auditable, and
built in the open.

## Getting Started

### Quick ways to contribute

- **Use Lind** and tell us what worked and what didn't.
- **Report bugs** or request features by opening a
  [GitHub issue](https://github.com/Lind-Project/lind-wasm/issues).
- **Improve the docs**, whether fixing a broken link or writing a new guide.
- **Star and share** the [repository](https://github.com/Lind-Project/lind-wasm)
  to help others find the project.
- To report a **security vulnerability**, please follow the
  [Security Policy](security.md) rather than opening a public issue.

### Set up your development environment

Lind currently targets the **AMD64** architecture and uses Docker to provide a
consistent build and test environment across host systems. You will need:

- [Docker](https://docs.docker.com/engine/install/)
- A recent [Rust toolchain](rust-toolchain.toml) (installed inside the dev
  container)
- Git

The fastest way to get a working environment is the prebuilt development image:

```bash
docker pull --platform=linux/amd64 securesystemslab/lind-wasm-dev
docker run --platform=linux/amd64 -it --privileged --ipc=host --init \
  --cap-add=SYS_PTRACE securesystemslab/lind-wasm-dev /bin/bash
```

For building images locally and other options, see the
[development setup guide](https://lind-project.github.io/lind-wasm/contribute/dev-container/).

### Find your first issue

- Browse the [issue tracker](https://github.com/Lind-Project/lind-wasm/issues),
  especially issues labeled *good first issue*.
- Not sure where to start? Open an issue describing what you'd like to work on,
  and a maintainer will help you scope it.

## Understanding the Codebase

Lind is a monorepo that combines in-house components with modified third-party
projects. The major pieces live under `src/`:

| Component    | Location         | Role                                                                 |
|--------------|------------------|----------------------------------------------------------------------|
| `3i`         | `src/threei`     | System-call mediation and routing layer for policy enforcement       |
| `rawposix`   | `src/rawposix`   | Trusted POSIX syscall implementations (the microvisor)               |
| `cage`       | `src/cage`       | The `Cage` execution context, including `vmmap` and signal handling  |
| `fdtables`   | `src/fdtables`   | File-descriptor table management for POSIX semantics                 |
| `typemap`    | `src/typemap`    | Shared data structures and type-conversion helpers                   |
| `sysdefs`    | `src/sysdefs`    | Shared syscall definitions and constants                             |
| `lind-boot`  | `src/lind-boot`  | Entry point wiring together the runtime, 3i, and RawPOSIX            |
| `glibc`      | `src/glibc`      | Modified glibc that compiles to WebAssembly and routes syscalls via 3i |
| `wasmtime`   | `src/wasmtime`   | Embedded WebAssembly runtime that executes cages                     |

For the concepts behind these components (cages, grates, 3i, the microvisor),
see the [internal documentation](https://lind-project.github.io/lind-wasm/internal/).

## What Can You Build?

There are many ways to contribute, depending on your interests:

- **Runtime & isolation** — Work on cages, memory management (`vmmap`), signals,
  and the isolation backends (WebAssembly today, with additional backends such
  as MPK on the roadmap).
- **System-call layer** — Extend or fix POSIX coverage in RawPOSIX and glibc, or
  improve 3i routing and policy handling.
- **Testing & quality** — Add unit, grate, and end-to-end tests, improve the test
  runner, or help stabilize flaky tests. See the
  [testing guide](https://lind-project.github.io/lind-wasm/contribute/testing/).
- **Tooling & CI** — Improve the build system, Docker images, benchmarks, and CI
  pipelines.
- **Documentation** — Improve guides, internal design docs, and getting-started
  material. See [adding to the docs](https://lind-project.github.io/lind-wasm/contribute/add-docs/).

## Pull Request Process

1. **Fork** the repository and create a branch for your change. For help, see
   GitHub's guide on
   [creating a pull request from a fork](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/proposing-changes-to-your-work-with-pull-requests/creating-a-pull-request-from-a-fork).
2. **Write tests** for new code, and make sure the existing tests pass locally.
3. **Format your code** (see [Code Style](#code-style)) and keep the docs
   up-to-date.
4. **Open a pull request**, filling in the PR template: describe the *purpose*,
   reference related *issues*, and add anything helpful for *reviewers*.
5. A maintainer will review your PR. Small, focused PRs are easier and faster to
   review than large ones. Please be responsive to review feedback.

### Pull request title format

We use [Conventional Commits](https://www.conventionalcommits.org/) style
prefixes, optionally with a scope, for example:

- `feat: add copy_file_range syscall`
- `fix(cage): correct vmmap wrap-around`
- `docs: rewrite rawposix architecture guide`
- `test: add deterministic memory tests`
- `ci: refactor release workflow`
- `chore: bump github actions`

Common prefixes: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`,
`build`, `ci`, `chore`, `revert`.

### Pull request description

A good description helps reviewers and future readers:

- Explain **what** changed and **why**.
- **Link related issues** (e.g., `Closes #1234`).
- Note any **user-facing or behavior changes**, including new syscalls or
  configuration.
- Include logs, benchmarks, or output where relevant.

## Code Style

Rust code follows the
[default Rust style](https://doc.rust-lang.org/style-guide/index.html) and should
be auto-formatted before submitting:

```bash
cargo fmt --all --manifest-path src/wasmtime/Cargo.toml
cargo fmt --all --manifest-path src/lind-boot/Cargo.toml
```

See the full [Rust style guide](https://lind-project.github.io/lind-wasm/contribute/styleguide/)
for details.

## Becoming a Maintainer

Maintainers are contributors who have shown sustained, high-quality involvement
in the project, through code implementation, review, documentation, and helping others. What
matters is consistent, thoughtful contribution and adherence to the
[Code of Conduct](https://lind-project.github.io/lind-wasm/community/conduct/),
not the raw number of PRs.

New maintainers are nominated by existing maintainers. A nomination is discussed
by the current maintainers, and confirmed by consensus. If you're interested in
taking on a larger role, the best path is to keep contributing and reviewing, and
to let a maintainer know.

## Code of Conduct

Participation in the Lind community is governed by our
[Code of Conduct](https://lind-project.github.io/lind-wasm/community/conduct/).
By participating, you agree to uphold it.

## License

By contributing to Lind, you agree that your contributions will be licensed under
the same license as the project. See [LICENSE](LICENSE) for details.

## Need Help?

- **Questions or ideas:** open a
  [GitHub issue](https://github.com/Lind-Project/lind-wasm/issues) or discussion.
- **Documentation:** browse the
  [Lind docs](https://lind-project.github.io/lind-wasm/).
- **Community:** see the
  [community page](https://lind-project.github.io/lind-wasm/community/) to connect
  with the team.
