For setting up Lind testing environment, see [Testing](../../docs/contribute/testing.md)

## Running Unit Tests

Unit tests can be run using the test harness:

```bash
/home/lind/lind-wasm/scripts/test/harnesses/wasmtestreport.py
```
To see available options:
```bash 
/home/lind/lind-wasm/scripts/test/harnesses/wasmtestreport.py --help
```

### Fork worker startup failure

`process_tests/fixtures/fork_spawn_failure.c` is a manual resource-exhaustion
test. Compile it statically with `lind_compile -s --output-dir tmp`, then run
the compiled guest through `timeout` in a container with `--pids-limit 6` and
`TOKIO_WORKER_THREADS=1`. The limit includes the `timeout` process, the
launcher, and its workers. The test verifies that a failed fork returns
`EAGAIN` and leaves no waitable child.

`process_tests/fixtures/fork_copy_failure.c` exercises a later failure. Compile
it as a static guest and compile `deny_process_vm_writev.c` as a native Linux
program. Run `deny_process_vm_writev build/lind-boot /tmp/fork_copy_failure.cwasm`
in the container; its seccomp filter forces the copy to fail. The guest checks
that the child and its shared-memory attachment were rolled back.

The Wasmtime multi-process unit test `fork_startup_tests` also drops the parent
startup receiver before and after thread registration and verifies child cleanup.
