# Claude Project Instructions

These instructions apply to all work in this repository.

## Use the Lind toolchain

- Use Lind's supported tools and entry points whenever they provide the required
  operation.
- Compile applications, libraries, grates, and LLVM bitcode through
  `scripts/lind_compile` (or the corresponding installed Lind command) instead of
  invoking `clang` directly.
- Run Lind programs through `scripts/lind_run` / `lind-wasm` instead of invoking
  `lind-boot` or Wasmtime directly, unless the task specifically tests that lower
  layer.
- Build runtime artifacts through repository targets such as `make lind-boot`,
  `make sysroot`, and `make all`; do not reconstruct their Cargo, compiler, linker,
  optimization, preload, or staging commands in scripts.
- Use a lower-level tool directly only when the Lind toolchain has a concrete,
  documented capability gap. Keep the exception narrow, explain the technical gap
  in the script or durable documentation, and ensure its ABI and flags remain
  consistent with the authoritative Lind path. If the capability is generally
  useful, prefer adding it to the Lind tool and consuming that interface.

## Write comments for the finished code

Comments must make the repository read as an intentional, coherent present-day
system. Explain only information that a future maintainer needs to understand the
code as it exists: purpose, invariants, ownership, non-obvious constraints, safety
requirements, and reasons behind durable design choices.

- Keep comments concise and natural. Do not restate the code line by line or narrate
  routine implementation details.
- Do not reference prompts, conversations, agents, temporary plans, scratch files,
  local experiments, debugging chronology, or the sequence used to implement the
  change.
- Do not mention a temporary plan or note merely because it informed the work. The
  code and comments must remain understandable after that material disappears.
- When removing an incorrect or unwanted feature, remove or rewrite its comments as
  well. Describe the resulting behavior directly; do not leave commentary saying
  that the old behavior was removed, disabled, reverted, or rejected.
- Do not preserve comparisons with obsolete implementations in ordinary code
  comments. If historical context is genuinely important to prevent a future design
  mistake, record the durable rationale in an issue, commit message, or maintained
  design document and let the code comment state the resulting invariant succinctly.
- Avoid comments tied to incidental names, the first function that exposed a bug, or
  the current test fixture. Describe the general semantic rule instead.
- Before finishing a change, reread every added or modified comment as if the task
  conversation and intermediate files never existed. Rewrite anything that depends
  on that missing context.

Examples:

- Prefer: `Reject signatures that exceed the portal's raw ABI slot capacity.`
- Avoid: `The temporary OpenBLAS plan says functions above six args are unsupported.`
- Prefer: `Calls without a registered handler trap in strict interposition mode.`
- Avoid: `The old local-fallback feature was removed because it broke isolation.`
