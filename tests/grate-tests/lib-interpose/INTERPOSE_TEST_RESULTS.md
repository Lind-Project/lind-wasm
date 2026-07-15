# Unit-test results under full-libc interposition

Status of `tests/unit-tests/**` run under the automatic full-libc interposition grate
(`libc_full_grate.cwasm`), with root-cause analysis of the failures.

- **Grate:** `tools/marshal-gen/gen_grate.py libc.marshal.json --lib-name libc --freestanding`
  → `libc_full_grate.c` → `lind_compile -s --compile-grate --fpcast-emu`.
- **Harness:** `scripts/harnesses/wasmtestreport.py --grate grates/libc_full_grate.cwasm --timeout 20`
- Each test's stdout/exit is compared to its native (`gcc`) baseline.

Reproduce a single test manually (orphan-safe):
```
scripts/lind_compile tests/unit-tests/<cat>/deterministic/<t>.c      # stages <t>.cwasm into lindfs
sudo timeout -s KILL 30 build/lind-boot --preload env=/lib/libc.cwasm \
    --preload env=/lib/libm.cwasm grates/libc_full_grate.cwasm <t>.cwasm
# add -DLIND_MARSHAL_DEBUG when rebuilding the grate to log every interposed call+return
```

---

## Summary

**Deterministic tests: 103 / 236 pass** (2026-07-01, grate = 1245 marshalled handlers).
History: 53/236 (initial working run) → 103/236 (after fd + startup-init force_local).

| category | pass / total |
|---|---|
| dylink_tests | 2 / 7 |
| file_tests | 44 / 72 |
| math_tests | 2 / 3 |
| memory_tests | 8 / 27 |
| networking_tests | 25 / 44 |
| process_tests | 19 / 56 |
| signal_tests | 1 / 24 |
| static_tests | 2 / 3 |
| **TOTAL** | **103 / 236** |

The big remaining sinks: **signal_tests** (1/24 — interposition disrupts async signal
delivery) and **process_tests** (19/56 — `fork`-heavy tests time out). See classes below.

---

## Why tests fail — root-cause classes

Failures split into two fundamentally different groups.

### Group A — functions that *cannot* be interposed (force_local is correct)

These touch **state that lives in, and is private to, the calling cage**. Running them in the
grate cage operates on the grate's copy of that state, not the app's. This is a correctness
necessity, not a limitation to fix — same reasoning for all of them.

| class | functions | why it breaks | status |
|---|---|---|---|
| **control-flow terminators** | `exit` `_exit` `_Exit` `exit_group` `quick_exit` `abort` | interposed, they run in the grate cage and *return* to the app → the app cage never dies → glibc `_exit` (`while(1){exit_group();exit();}`) spins forever | force_local ✅ |
| **non-local jumps** | `setjmp` (address can't be taken) `longjmp` (restores a `jmp_buf` saved on the *caller's* stack) | jump target/stack is caller-local | force_local ✅ |
| **image replacement** | `exec*` | replaces the *calling* cage's image | force_local ✅ |
| **variadic** | `printf`/`scanf` family | the marshal spec can't describe `...` args | force_local ✅ |
| **locale** | `*locale*` | TLS-bound locale state; also drags unresolved TLS at static-link | force_local ✅ |
| **startup / runtime init** | `__libc_setup_tls` `__ctype_init` `__wasi_init_tp` | called by `__libc_start_main` to set up the *caller's* TLS / ctype tables / thread pointer; interposed, they init the grate cage, leaving the app half-initialized. Symptom: whole-number `%f` prints `4` instead of `4.000000000` (integer path of `__printf_fp` depends on this state) | force_local ✅ (confirmed) |
| **stdio streams** | `puts` `fputs` `fwrite` `putchar` `fputc` `putc` `fflush` `_IO_*` … | write through the *caller's* `FILE` buffer (`stdout`); interposed, output goes through the grate's separate `stdout` buffer → app-side (force_local `printf`) and grate-side (`puts`) output flush independently → **scrambled order**. Same reason `printf` is already excluded | ⚠️ under discussion |

### Group B — functions that *should* work but don't yet

| class | functions | why it breaks | status |
|---|---|---|---|
| **file descriptors** | `read` `write` `close` `lseek` `dup*` `fstat*` `socket` `accept*` `*at` … | an fd is a per-cage handle (index into that cage's fdtable), passed as a plain `int`. The grate's libc wrapper re-issues the syscall via `MAKE_LEGACY_SYSCALL`, which hard-codes every arg's cageid to `__lind_cageid` (= grate) → RawPOSIX resolves the fd in the grate's fdtable → `EBADF`. The 3i model *does* support this (`convert_fd_to_host(fd, arg_cageid, cageid)` resolves in the fd's owner cage); the fix is to preserve the fd argument's owner-cageid into the underlying syscall instead of letting glibc default it. **Deferred — real fix pending.** | temporarily force_local |

### Other failure buckets (harness/environment, not marshalling)

| bucket | meaning |
|---|---|
| **Timeout** | test blocks/hangs under interposition — dominated by signal-delivery, `fork`, and blocking socket tests |
| **Failure_native_running** | the *native* baseline failed to run (unrelated to the grate) |
| **Unknown_Failure** | non-zero exit / crash not yet root-caused |

---

## Result (latest full run)

### Failure-type totals

- **73** Unknown_Failure — crash / non-zero exit — needs analysis
- **28** Lind_wasm_Timeout — Timeout — blocks/hangs (signal/fork/socket)
- **21** Output_mismatch — output differs — needs analysis
- **11** Failure_native_running — native baseline failed (not the grate)

### Per-test detail


<details><summary><b>dylink_tests</b> — 2/7</summary>

- ✅ `basic.c`
- ❌ `dlopen_fork.c` — output differs — needs analysis
- ❌ `dlopen_thread.c` — Timeout — blocks/hangs (signal/fork/socket)
- ❌ `double_fork_dlopen.c` — output differs — needs analysis
- ❌ `fork_dlopen.c` — output differs — needs analysis
- ✅ `longjmp_dlopen.c`
- ❌ `rdynamic_main.c` — output differs — needs analysis

</details>

<details><summary><b>file_tests</b> — 44/72</summary>

- ✅ `chartests.c`
- ❌ `chdir_getcwd.c` — crash / non-zero exit — needs analysis
- ❌ `chmod.c` — crash / non-zero exit — needs analysis
- ✅ `clock_gettime_highlevel.c`
- ✅ `clock_gettime_simple.c`
- ✅ `cloexec.c`
- ❌ `close.c` — native baseline failed (not the grate)
- ✅ `creat_access.c`
- ✅ `doubleclose.c`
- ✅ `dup.c`
- ❌ `dup2.c` — output differs — needs analysis
- ❌ `dup3.c` — crash / non-zero exit — needs analysis
- ✅ `dupwrite.c`
- ❌ `etc_conf.c` — crash / non-zero exit — needs analysis
- ❌ `faccessat.c` — native baseline failed (not the grate)
- ✅ `fchdir.c`
- ❌ `fchmod.c` — output differs — needs analysis
- ✅ `fchmodat.c`
- ✅ `fcntl.c`
- ✅ `fcntl_dupfd.c`
- ✅ `fdatasync.c`
- ✅ `filetest.c`
- ✅ `filetest1000.c`
- ❌ `flock.c` — Timeout — blocks/hangs (signal/fork/socket)
- ✅ `fstat.c`
- ❌ `fstatfs.c` — native baseline failed (not the grate)
- ✅ `fsync.c`
- ✅ `ftruncate.c`
- ✅ `getcwd.c`
- ✅ `getcwd_null.c`
- ✅ `getpgid.c`
- ✅ `getrandom.c`
- ❌ `ioctl.c` — output differs — needs analysis
- ❌ `link.c` — crash / non-zero exit — needs analysis
- ❌ `locale_test.c` — crash / non-zero exit — needs analysis
- ✅ `lseek.c`
- ❌ `lstat.c` — output differs — needs analysis
- ✅ `mkdir_rmdir.c`
- ❌ `mkfifo_test.c` — crash / non-zero exit — needs analysis
- ❌ `mknod.c` — crash / non-zero exit — needs analysis
- ❌ `nocancel_io.c` — native baseline failed (not the grate)
- ✅ `open.c`
- ✅ `openat.c`
- ✅ `path_conversion_safety.c`
- ❌ `ppoll.c` — crash / non-zero exit — needs analysis
- ❌ `pread_pwrite.c` — output differs — needs analysis
- ❌ `preadv_pwritev.c` — crash / non-zero exit — needs analysis
- ✅ `printf.c`
- ✅ `prlimit64.c`
- ✅ `read.c`
- ✅ `readbytes.c`
- ❌ `readdir_basic.c` — native baseline failed (not the grate)
- ❌ `readlink.c` — output differs — needs analysis
- ✅ `readlinkat.c`
- ✅ `readv_writev_test.c`
- ✅ `rename.c`
- ❌ `renameat.c` — native baseline failed (not the grate)
- ✅ `sc-writev.c`
- ✅ `stat.c`
- ❌ `statfs.c` — native baseline failed (not the grate)
- ❌ `symlink.c` — native baseline failed (not the grate)
- ✅ `sync_file_range.c`
- ✅ `timespec_time_t_compat.c`
- ✅ `trailing_slash.c`
- ✅ `truncate.c`
- ❌ `unlink.c` — native baseline failed (not the grate)
- ❌ `unlinkat.c` — output differs — needs analysis
- ❌ `utimensat.c` — crash / non-zero exit — needs analysis
- ✅ `write.c`
- ✅ `writeloop.c`
- ✅ `writepartial.c`
- ✅ `writev.c`

</details>

<details><summary><b>math_tests</b> — 2/3</summary>

- ✅ `math_link_smoke.c`
- ❌ `math_tests.c` — output differs — needs analysis
- ✅ `printf_float.c`

</details>

<details><summary><b>memory_tests</b> — 8/27</summary>

- ❌ `brk.c` — crash / non-zero exit — needs analysis
- ❌ `fork_large_memory.c` — crash / non-zero exit — needs analysis
- ✅ `malloc.c`
- ✅ `malloc_large.c`
- ✅ `memcpy.c`
- ❌ `memory_error_test.c` — output differs — needs analysis
- ❌ `mmap.c` — crash / non-zero exit — needs analysis
- ✅ `mmap_address_truncation.c`
- ❌ `mmap_aligned.c` — crash / non-zero exit — needs analysis
- ❌ `mmap_complicated.c` — crash / non-zero exit — needs analysis
- ❌ `mmap_file.c` — crash / non-zero exit — needs analysis
- ❌ `mmap_shared.c` — crash / non-zero exit — needs analysis
- ❌ `mmaptest.c` — crash / non-zero exit — needs analysis
- ❌ `mprotect.c` — crash / non-zero exit — needs analysis
- ❌ `mprotect_boundary.c` — crash / non-zero exit — needs analysis
- ❌ `mprotect_end_region.c` — crash / non-zero exit — needs analysis
- ❌ `mprotect_middle_region.c` — crash / non-zero exit — needs analysis
- ❌ `mprotect_multiple_times.c` — crash / non-zero exit — needs analysis
- ❌ `mprotect_same_value.c` — crash / non-zero exit — needs analysis
- ❌ `mprotect_spanning_regions.c` — crash / non-zero exit — needs analysis
- ❌ `munmap_adjacent_shm.c` — crash / non-zero exit — needs analysis
- ✅ `sbrk.c`
- ❌ `segfault.c` — crash / non-zero exit — needs analysis
- ❌ `shm.c` — crash / non-zero exit — needs analysis
- ✅ `shmtest.c`
- ✅ `thread_malloc_sequential.c`
- ✅ `vtable.c`

</details>

<details><summary><b>networking_tests</b> — 25/44</summary>

- ✅ `accept4.c`
- ❌ `dns_resolve_test.c` — crash / non-zero exit — needs analysis
- ✅ `dnstest.c`
- ✅ `epoll_badfd.c`
- ✅ `epoll_edge_triggered.c`
- ❌ `epollcreate1.c` — output differs — needs analysis
- ✅ `error_handling_net.c`
- ❌ `getaddrinfo_test.c` — crash / non-zero exit — needs analysis
- ✅ `getaddrinfo_unspec.c`
- ✅ `gethostname.c`
- ✅ `getifaddrs.c`
- ✅ `getsockname.c`
- ✅ `getsockopt.c`
- ✅ `ipv6_basic.c`
- ✅ `makepipe.c`
- ❌ `nonblocking_eagain.c` — output differs — needs analysis
- ✅ `pipe.c`
- ✅ `pipe2.c`
- ❌ `pipeinput.c` — crash / non-zero exit — needs analysis
- ❌ `pipeinput2.c` — crash / non-zero exit — needs analysis
- ❌ `pipeonestring.c` — crash / non-zero exit — needs analysis
- ❌ `pipepong.c` — Timeout — blocks/hangs (signal/fork/socket)
- ✅ `pipewrite.c`
- ✅ `poll.c`
- ✅ `recvfrom-sendto.c`
- ✅ `sendmsg_recvmsg_test.c`
- ✅ `serverclient.c`
- ✅ `shutdown.c`
- ❌ `shutdown_fork.c` — crash / non-zero exit — needs analysis
- ❌ `simple-select.c` — crash / non-zero exit — needs analysis
- ❌ `simple_epoll.c` — output differs — needs analysis
- ✅ `socket.c`
- ✅ `socket_cloexec.c`
- ❌ `socket_options_advanced.c` — output differs — needs analysis
- ✅ `socketepoll.c`
- ✅ `socketpair.c`
- ❌ `socketselect.c` — crash / non-zero exit — needs analysis
- ❌ `udp_send_recv.c` — crash / non-zero exit — needs analysis
- ✅ `uds-getsockname.c`
- ❌ `uds-nb-select.c` — Timeout — blocks/hangs (signal/fork/socket)
- ❌ `uds-serverclient.c` — crash / non-zero exit — needs analysis
- ❌ `uds-socketselect.c` — crash / non-zero exit — needs analysis
- ❌ `uds_listen_poll.c` — crash / non-zero exit — needs analysis
- ❌ `writev_socket.c` — crash / non-zero exit — needs analysis

</details>

<details><summary><b>process_tests</b> — 19/56</summary>

- ❌ `barrier_test.c` — Timeout — blocks/hangs (signal/fork/socket)
- ✅ `chain_thread.c`
- ✅ `ctor_syscall_test.c`
- ❌ `cxa_atexit_test.c` — output differs — needs analysis
- ✅ `exec_non_utf8.c`
- ✅ `execve_shebang.c`
- ✅ `exit.c`
- ❌ `exit_failure.c` — Timeout — blocks/hangs (signal/fork/socket)
- ❌ `exit_group_thread.c` — crash / non-zero exit — needs analysis
- ❌ `exit_status_first_wins.c` — crash / non-zero exit — needs analysis
- ❌ `flockfile_test.c` — Timeout — blocks/hangs (signal/fork/socket)
- ❌ `fork2malloc.c` — crash / non-zero exit — needs analysis
- ❌ `fork_select.c` — crash / non-zero exit — needs analysis
- ❌ `fork_simple.c` — crash / non-zero exit — needs analysis
- ❌ `fork_syscall.c` — crash / non-zero exit — needs analysis
- ❌ `fork_tls_ctype.c` — crash / non-zero exit — needs analysis
- ❌ `forkandopen.c` — crash / non-zero exit — needs analysis
- ❌ `forkdup.c` — Timeout — blocks/hangs (signal/fork/socket)
- ❌ `forkexecuid.c` — native baseline failed (not the grate)
- ❌ `forkexecv-arg.c` — Timeout — blocks/hangs (signal/fork/socket)
- ❌ `forkexecv.c` — Timeout — blocks/hangs (signal/fork/socket)
- ❌ `forkfiles.c` — crash / non-zero exit — needs analysis
- ❌ `forkmalloc.c` — crash / non-zero exit — needs analysis
- ❌ `forknodup.c` — crash / non-zero exit — needs analysis
- ✅ `function-ptr.c`
- ❌ `getegid_syscall.c` — crash / non-zero exit — needs analysis
- ❌ `getgid_syscall.c` — crash / non-zero exit — needs analysis
- ✅ `getpid.c`
- ❌ `getpid_syscall.c` — crash / non-zero exit — needs analysis
- ❌ `getppid.c` — Timeout — blocks/hangs (signal/fork/socket)
- ❌ `getppid_syscall.c` — crash / non-zero exit — needs analysis
- ❌ `getuid.c` — native baseline failed (not the grate)
- ❌ `getuid_syscall.c` — crash / non-zero exit — needs analysis
- ✅ `hello-arg.c`
- ✅ `hello.c`
- ❌ `longjmp.c` — output differs — needs analysis
- ❌ `mutex.c` — Timeout — blocks/hangs (signal/fork/socket)
- ❌ `printf_deadlock_smoke.c` — Timeout — blocks/hangs (signal/fork/socket)
- ❌ `printf_thread_test.c` — Timeout — blocks/hangs (signal/fork/socket)
- ✅ `sem_forks.c`
- ❌ `setjmp_edge.c` — output differs — needs analysis
- ✅ `setsid.c`
- ❌ `template.c` — crash / non-zero exit — needs analysis
- ❌ `test_crossmodule_longjmp.c` — crash / non-zero exit — needs analysis
- ✅ `test_exec_nofork.c`
- ✅ `test_unlink_open_file.c`
- ✅ `thread-guard.c`
- ✅ `thread-test.c`
- ✅ `thread.c`
- ✅ `thread_cageid_race.c`
- ✅ `tls_test.c`
- ✅ `uname.c`
- ❌ `wait.c` — Timeout — blocks/hangs (signal/fork/socket)
- ❌ `waitpid_anychild.c` — Timeout — blocks/hangs (signal/fork/socket)
- ❌ `waitpid_syscall.c` — crash / non-zero exit — needs analysis
- ❌ `waitpid_wnohang.c` — crash / non-zero exit — needs analysis

</details>

<details><summary><b>signal_tests</b> — 1/24</summary>

- ❌ `alarm.c` — Timeout — blocks/hangs (signal/fork/socket)
- ❌ `eintr_fork_signal.c` — Timeout — blocks/hangs (signal/fork/socket)
- ❌ `kill.c` — crash / non-zero exit — needs analysis
- ❌ `pause_test.c` — crash / non-zero exit — needs analysis
- ❌ `setitimer.c` — Timeout — blocks/hangs (signal/fork/socket)
- ❌ `sigalrm.c` — Timeout — blocks/hangs (signal/fork/socket)
- ❌ `sigaltstack.c` — crash / non-zero exit — needs analysis
- ❌ `sigchld.c` — output differs — needs analysis
- ❌ `signal-fork.c` — crash / non-zero exit — needs analysis
- ❌ `signal-simple.c` — Timeout — blocks/hangs (signal/fork/socket)
- ❌ `signal_SIGCHLD.c` — crash / non-zero exit — needs analysis
- ❌ `signal_fork.c` — Timeout — blocks/hangs (signal/fork/socket)
- ❌ `signal_int_ignored.c` — crash / non-zero exit — needs analysis
- ❌ `signal_kill_cleanup.c` — crash / non-zero exit — needs analysis
- ❌ `signal_procmask.c` — Timeout — blocks/hangs (signal/fork/socket)
- ❌ `signal_read_interrupt.c` — Timeout — blocks/hangs (signal/fork/socket)
- ❌ `signal_recursive.c` — Timeout — blocks/hangs (signal/fork/socket)
- ❌ `signal_sa_mask.c` — Timeout — blocks/hangs (signal/fork/socket)
- ❌ `signal_select_interrupt.c` — crash / non-zero exit — needs analysis
- ❌ `signal_write_interrupt.c` — Timeout — blocks/hangs (signal/fork/socket)
- ✅ `sigpipe.c`
- ❌ `sigprocmask.c` — Timeout — blocks/hangs (signal/fork/socket)
- ❌ `sigsuspend_test.c` — crash / non-zero exit — needs analysis
- ❌ `test_sigsetjmp.c` — crash / non-zero exit — needs analysis

</details>

<details><summary><b>static_tests</b> — 2/3</summary>

- ❌ `fork_simple.c` — crash / non-zero exit — needs analysis
- ✅ `thread.c`
- ✅ `tls_test.c`

</details>


---

## How to update this file

1. Re-run: `python3 scripts/harnesses/wasmtestreport.py --grate grates/libc_full_grate.cwasm --timeout 20 --output <out>`
2. Regenerate the tables from `<out>.json` (`deterministic.test_cases`: `status`, `error_type`).
3. For each still-unexplained failure, deep-dive (native-vs-interposed diff + `-DLIND_MARSHAL_DEBUG`
   trace) and record the root cause / class above.
