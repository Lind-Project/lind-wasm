Lind-MPK dispatch benchmark runner
=================================
From the repository root on the host, run:

python3 tools/mpk-dispatch-benchmark/run_mpk_dispatch.py \
  --container cranky_keldysh --cpu 8 --output build/mpk-dispatch-results

Output under build/ is ignored by git. Choose a new output directory for each
experiment. Keep these five source/script files together if copying the tool
elsewhere.

The container must already be running and have the Lind development toolchain,
modified native glibc, Cargo dependencies, and gsfork-elf available. No packages
are installed. The host needs Python 3.9+ and Docker access.

Options:
  --container   dev container name (default cranky_keldysh)
  --repo        repository path INSIDE container (default /home/lind/lind-wasm)
  --rewriter    executable INSIDE container
                (default /home/lind/rewriter/.venv/bin/gsfork-elf)
  --cpu         allowed logical CPU (default 8)
  --runs        fresh processes, even and >=20 (default 40)
  --perf-bin    optional real perf executable INSIDE container; enables profiling
  --output      NEW HOST directory (default mpk-dispatch-results)

The runner:
1. Builds make lind-boot with no extra/debug features and verifies the optimized
   release profile and identical installed/release runtime bytes. It replaces
   build/lind-boot with the release build, so do not run alongside a runtime
   rebuild or another benchmark.
2. Copies sources into a unique container temporary directory, compiles native
   cage/grate at -O2 -Werror, and performs normal gsfork binary rewriting.
3. Calibrates the host TSC against CLOCK_MONOTONIC_RAW and CPUID.
4. Runs 40 fresh processes pinned to one CPU, with 20,000 warmup calls,
   10,000 individual measurements, and 1,000 batches per process.
5. Uses process-level 10,000-resample bootstrap CIs, drift checks, and comparison
   of randomized 100/1000-call batch sizes. No samples are trimmed.
6. Saves source snapshots/hashes, build logs, provenance, raw CSVs, analysis
   output, and results.json in the host output directory.
7. Removes its named temporary container sources and binaries.

The handler returns 10 without forwarding or logging. Parent wait logic sleeps
between WNOHANG checks to avoid the runtime's blocking-wait busy-yield artifact.
Timing uses cage-side LFENCE/RDTSC/LFENCE: plain unfenced RDTSC could reorder
across the measured call.

Interpretation:
- Measures full libc geteuid + supervisor/3i + grate dispatch/return round trip,
  NOT one-way dispatch latency.
- Reports both raw timings and local-control-subtracted incremental batch cost.
- TSC ticks are reference-clock ticks, not turbo-dependent core cycles.
- CPU pinning prevents migration, not interrupts, SMT contention, or host load.
- Batch averages do not describe individual-call tails.
- Small drift and correlations are retained and reported, not hidden.
- Matching exact nanoseconds requires a comparable CPU, runtime/toolchain,
  glibc, frequency policy, and host load; reproducibility here means repeatable
  build/measurement/analysis, not an assertion that every run has identical cost.

No repository source files are edited by the runner. The optimized runtime
remains installed. Existing output directories are rejected to prevent mixing
experiments or overwriting results.

Perf analysis
-------------
To repeat hardware-counter and cycle-sampling analysis:

python3 tools/mpk-dispatch-benchmark/run_mpk_dispatch.py \
  --container cranky_keldysh --cpu 8 --runs 20 \
  --perf-bin /usr/lib/linux-tools/5.15.0-202-generic/perf \
  --output build/mpk-dispatch-perf

The installed /usr/bin/perf wrapper in this container looks for a missing
kernel-specific executable. The actual 5.15 perf executable works on the host's
6.11 kernel. Set --perf-bin to your container's actual executable. Profiling
requires PMU and syscall tracepoint access (this development container is
privileged). The runner probes PMU access before building.

Perf mode adds two long invocations of the SAME cage/grate test (1,000 batches
of 100,000 calls each): perf stat user counters and perf record cycles at 999Hz
including kernel execution. A shorter invocation counts signal-mask, arch_prctl,
geteuid, and sched_yield host syscalls. It saves raw benchmark output, perf.data,
counter results, self-overhead reports, and runtime instruction annotations.
Allow about 90 seconds in addition to the normal build/benchmark.

Perf percentages are sampled CPU-cycle attribution, NOT additive exact
nanoseconds or inclusive call-tree costs. Call stacks are deliberately not
unwound across custom MPK stack/FS transitions. Ordinary perf reports may leave
guest ELF code as anonymous/JIT or unknown: the runtime maps it manually.
For the original analyzed run, guest-code ranges were resolved using live
/proc/PID/maps and exact executable-segment byte comparison; see
perf-analysis.txt for the verified breakdown and limitations.
