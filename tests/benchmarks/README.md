# Lind Benchmarks

This directory contains microbenchmarks used by `scripts/benchrunner.py`.
Each benchmark prints results in a tab-delimited format so the runner can parse them.

Sample outputs:

```bash
lind@232affd4dc4d:~/lind-wasm$ ./scripts/benchrunner.py fs_read imfs-grate.fs_read.grate
Running:  /home/lind/lind-wasm/tests/benchmarks/fs_read.c
Running:  /home/lind/lind-wasm/tests/benchmarks/imfs-grate.fs_read.grate
TEST  PARAM  LINUX (ns)   LIND (ns)    GRATE (ns)    ITERATIONS  DESCRIPTION
----  -----  -----------  -----------  ------------  ----------  ----------------------------------------
Read  1      214 (1.000)  414 (1.935)  824 (3.850)   1000000     Issues pread() for buffer of size PARAM. / Grate used: In-Memory File System.
Read  1024   226 (1.000)  436 (1.929)  855 (3.783)   1000000
Read  4096   244 (1.000)  433 (1.775)  930 (3.811)   1000000
Read  10240  359 (1.000)  532 (1.482)  1046 (2.914) 1000
```

```bash
lind@232affd4dc4d:~/lind-wasm$ ./scripts/benchrunner.py -o results.csv fs_read imfs-grate.fs_read.grate
Running:  /home/lind/lind-wasm/tests/benchmarks/fs_read.c
Running:  /home/lind/lind-wasm/tests/benchmarks/imfs-grate.fs_read.grate
TEST,PARAM,LINUX (ns),LIND (ns),GRATE (ns),ITERATIONS,DESCRIPTION
Read,1,"214 (1.000)","414 (1.935)","824 (3.850)",1000000,"Issues pread() for buffer of size PARAM. / Grate used: In-Memory File System."
Read,1024,"226 (1.000)","436 (1.929)","855 (3.783)",1000000,
Read,4096,"244 (1.000)","433 (1.775)","930 (3.811)",1000000,
Read,10240,"359 (1.000)","532 (1.482)","1046 (2.914)",1000,
```

## Output Format

Each benchmark uses `bench.c/bench.h` and prints exactly one line per data point:

```
<test>\t<param>\t<loops>\t<avg_ns>
```

Fields:
- `test`: human-readable label (string)
- `param`: an integer parameter for the test (size, id, etc.)
- `loops`: number of iterations used to compute the average
- `avg_ns`: average time per iteration in **nanoseconds**

The helper function `emit_result()` in `bench.c` enforces this format.

Descriptions (optional) can be placed at the top of a test file in the form:

```
// DESCRIPTION: Issues pread() for buffer of size PARAM.
```

For `.grate` tests, the description is formed by concatenating the `.c` test
description(s) with the `.grate` file description. The console table prints the
description only on the first row for a given test; CSV mirrors the console output.

## How Timing Works

Benchmarks record a start and end timestamp with `gettimens()`
and compute `avg = (end - start) / loops`.
`gettimens()` uses `clock_gettime(CLOCK_MONOTONIC)` to avoid time jumps.

## Running

From repo root:

```
python3 scripts/benchrunner.py
```

Optional:
- `python3 scripts/benchrunner.py fs_ imfs_` runs only tests whose filename starts with `fs_` or `imfs_`
- `python3 scripts/benchrunner.py --out results.csv` writes CSV instead of a table

## Benchmark Types

### Syscall Tests
Examples: `sys_close.c`, `sys_geteuid.c`
These call a single syscall in a tight loop and report average time per call.

### File System Tests
Examples: `fs_read.c`, `fs_write.c`
These vary the operation size (`param` is bytes) and adjust loop counts
to keep runtimes reasonable.

### IMFS Tests
Example: `imfs_read.c`
These call IMFS functions directly to estimate in-memory FS overhead
without routing through a grate.

### IPC Tests
Examples: `ipc_pipe.c`, `ipc_uds.c`
These measure round-trip time (RTT) for different message sizes.
`param` is message size in bytes.

### Microbench Harnesses
Directory: `tests/benchmarks/microbenches/`
These are low-level syscall routing tests used with `lind-boot --perf`.
They do not emit `emit_result()` lines and are not parsed by `benchrunner.py`.

## Grate Tests

Files ending in `.grate` represent a grate-backed benchmark.
`benchrunner.py` interprets the filename as a dot-separated list of inputs.
For example:

```
imfs-grate.fs_read.grate
```

This means:
- fetch `examples/imfs-grate/` from the [lind-wasm-example-grates](https://github.com/Lind-Project/lind-wasm-example-grates) repo  
- compile the grate via `compile_grate.sh`
- compile `tests/benchmarks/fs_read.c` as a cage
- run `lind-boot imfs_grate fs_read` so the syscall is interposed by the grate

The `.grate` tests files are used to encode the expected order to launch grates and cages,
and can optionally include a `// DESCRIPTION:` line to describe the intended workflow being benchmarked. 

## Adding a New Test

### Regular (non-grate) tests
1. Create a new `.c` file in `tests/benchmarks/`.
2. Include `bench.h` and use `gettimens()` + `emit_result(...)`.
3. Make sure any required artifacts (files, sockets, temp paths) are created and
   cleaned up inside the test itself.
4. Use a consistent `test` label in `emit_result(...)`. Results with the same label and
  `param` are aggregated across platforms during a single `benchrunner.py` run.

### Grate tests
Taking the example of adding a `geteuid_grate` test:
1. Ensure that the grate exists in the `lind-wasm-example-grates` repo with a valid `compile_grate.sh` script.
2. Add a file ending with `.grate`. The name of this file encodes the order of execution for this test. Name the file `geteuid-grate.geteuid.grate` which would run `lind-boot geteuid_grate.cwasm geteuid.cwasm`. You can add a `// DESCRIPTION:` line here for grate-specific context.
3. For components that are not grates, these files must already be present in `tests/benchmarks/`. In this case, `geteuid.c` must already exist. 

### Aggregation:
- Results are keyed by the `test` label and `param`.
- If you have multiple binaries (linux/lind/grate) that emit the same `(label, param)` the
  results are grouped together in the final table.
