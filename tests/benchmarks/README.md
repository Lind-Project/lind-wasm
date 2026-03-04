# Lind Benchmarks

This directory contains microbenchmarks used by `scripts/benchrunner.py`.
Each benchmark prints results in a tab-delimited format so the runner can parse them.

Sample outputs:

```bash
lind@232affd4dc4d:~/lind-wasm$ ./scripts/benchrunner.py fs_read imfs_grate.fs_read.grate
Running:  /home/lind/lind-wasm/tests/benchmarks/fs_read.c
Running:  /home/lind/lind-wasm/tests/benchmarks/imfs_grate.fs_read.grate
TEST  PARAM  LINUX (ns)  LIND (ns)    GRATE (ns)    ITERATIONS
----  -----  ----------  -----------  ------------  ----------
Read  1      240         451 (1.879)  1102 (4.592)  1000000
Read  1024   252         461 (1.829)  1102 (4.373)  1000000
Read  4096   267         475 (1.779)  1149 (4.303)  1000000
Read  10240  389         586 (1.506)  1348 (3.465)  1000
```

```bash
lind@232affd4dc4d:~/lind-wasm$ ./scripts/benchrunner.py -o result.json fs_read imfs_grate.fs_read.grate && jq . result.json
Running:  /home/lind/lind-wasm/tests/benchmarks/fs_read.c
Running:  /home/lind/lind-wasm/tests/benchmarks/imfs_grate.fs_read.grate
{
  "Read": {
    "1": {
      "grate": "1043",
      "lind": "448",
      "linux": "237",
      "loops": "1000000"
    },
    "1024": {
      "grate": "1038",
      "lind": "457",
      "linux": "250",
      "loops": "1000000"
    },
    "4096": {
      "grate": "1088",
      "lind": "469",
      "linux": "265",
      "loops": "1000000"
    },
    "10240": {
      "grate": "1273",
      "lind": "712",
      "linux": "383",
      "loops": "1000"
    }
  }
}
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
- `python3 scripts/benchrunner.py --out results.json` writes JSON instead of a table

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
- fetch `examples/imfs-grate/` from the upstream grates repo
- copy it into `tests/benchmarks/imfs-grate/`
- compile that grate via `compile_grate.sh`
- compile `tests/benchmarks/fs_read.c` as a cage
- run `lind-boot imfs_grate fs_read` so the syscall is interposed by the grate

Grate directories contain `compile_grate.sh` which use `lind_compile --compile-grate`.

The `.grate` tests files are empty and are used only to encode the expected order to launch grates and cages.

Notes:
- Grates are sourced from `https://github.com/Lind-Project/lind-wasm-example-grates` under `examples/`.

## Adding a New Test

### Regular (non-grate) tests
1. Create a new `.c` file in `tests/benchmarks/`.
2. Include `bench.h` and use `gettimens()` + `emit_result(...)`.
3. Make sure any required artifacts (files, sockets, temp paths) are created and
   cleaned up inside the test itself.

Notes:
- The test runner does not manage artifacts for you. For example, `fs_read.c` creates
  a file before reading and unlinks it afterwards.
- Use a consistent `test` label in `emit_result(...)`. Results with the same label and
  `param` are aggregated across platforms during a single `benchrunner.py` run.

### Grate tests
Taking the example of adding a `geteuid_grate` test:
1. Add the grate to the upstream repo under `examples/` and name it with `-` (for example `geteuid-grate`).
2. Add an empty file ending with `.grate`. The name of this file encodes the order of execution for this test. Name the file `geteuid-grate.geteuid.grate` which would run `lind-boot geteuid_grate.cwasm geteuid.cwasm`
3. For components that are not grates, these files must already be present in `tests/benchmarks/`. In this case, `geteuid.c` must already exist. 


### Aggregation:
- Results are keyed by the `test` label and `param`.
- If you have multiple binaries (linux/lind/grate) that emit the same `(label, param)` the
  results are grouped together in the final table.

## Notes

- `benchrunner.py` runs each benchmark twice: once under Lind (`sudo lind-boot`) and once natively.
- Native binaries are compiled into `lindfs/` so paths are consistent with `lind-boot`’s chroot.
