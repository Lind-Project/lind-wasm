# RawPOSIX

RawPOSIX is the POSIX syscall implementation layer of GrateOS. It implements
file system, networking, signal, and process-management syscalls in userspace,
backed by the host kernel, and is dispatched to through the 3i interface by
`grateos-boot` and the Wasmtime-based runtime.

## Layout

- `src/` — syscall implementations, grouped by subsystem
- Depends on the sibling crates `sysdefs` (shared constants/types), `fdtables`
  (file-descriptor virtualization), `cage`, and `typemap`.

## Building and testing

RawPOSIX is built as part of the top-level build:

```bash
make build
```

See the repository root `Makefile` for individual targets (`grateos-boot`,
`grateosfs`, `sysroot`) and `scripts/test/` for the test harnesses.
