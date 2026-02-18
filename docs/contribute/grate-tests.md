# Grate tests

Grate tests are designed to validate a special feature in lind-wasm: the grate mechanism. The concept of a grate is defined in internal/3i.

Grate tests ensure that the grate–cage interaction model behaves correctly under the expected fork/exec execution semantics.

## Structure of a Grate Test

Each grate test must contain at least two source files:

- A grate file
- A cage file

### Grate tests Format

**Naming Convention**

```sh
<cage_name>.c
<cage_name>_grate.c
```

Example:

```sh
hello.c
hello_grate.c
```

**Test Requirements**

Each grate test must:

- Determine test success internally
- Exit with EXIT_FAILURE if the test fails
- Exit normally (e.g., EXIT_SUCCESS) on success

Tests are expected to be self-validating.

**Execution Model**

Grate tests must follow the fork/exec model:

- The grate file executes first.
- The grate performs a fork().
- The child process becomes the cage.
- The grate registers the necessary handler(s).
- The child performs exec() to execute the cage file.

## How to Build and Run

### Prerequisite

Make sure the lind-wasm runtime has already been compiled.

**Step 1 — Compile the Grate**

```sh
lind-clang --compile-grate <cage_name>_grate.c
```

This produces `<cage_name>_grate.wasm`

**Step 2 — Compile the Cage**

```sh
lind-clang <cage_name>.c
```

This produces `<cage_name>.wasm`

**Step 3 — Run the Test**

```sh
lind-wasm <cage_name>_grate.wasm <cage_name>.wasm
```
