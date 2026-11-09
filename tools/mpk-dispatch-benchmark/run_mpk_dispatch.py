#!/usr/bin/env python3
"""Build and benchmark Lind-MPK in a running development container."""

import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--container", default="cranky_keldysh")
    parser.add_argument("--repo", default="/home/lind/lind-wasm",
                        help="absolute repository path inside the container")
    parser.add_argument("--rewriter", default="/home/lind/rewriter/.venv/bin/gsfork-elf",
                        help="gsfork-elf executable inside the container")
    parser.add_argument("--cpu", type=int, default=8)
    parser.add_argument("--runs", type=int, default=40)
    parser.add_argument("--perf-bin",
                        help="optional real perf executable inside container (not a missing-kernel wrapper)")
    parser.add_argument("--output", type=Path, default=Path("mpk-dispatch-results"),
                        help="new host directory for sources, logs, raw CSVs, and results.json")
    args = parser.parse_args()
    if not PurePosixPath(args.repo).is_absolute():
        parser.error("--repo must be absolute")
    if args.cpu < 0 or args.runs < 20 or args.runs % 2:
        parser.error("--cpu must be nonnegative; --runs must be even and at least 20")
    output = args.output.resolve()
    if output.exists():
        parser.error("--output already exists; choose a fresh directory")
    sources = Path(__file__).resolve().parent
    companions = ["geteuid_dispatch.c", "geteuid_dispatch_grate.c",
                  "tsc_frequency.c", "benchmark_dispatch.py"]
    for name in companions:
        if not (sources / name).is_file():
            parser.error(f"Missing companion source: {sources / name}")
    prefix = ["docker", "exec", "-u", "root", "-w", args.repo, args.container]

    def run(command, log=None):
        result = subprocess.run(command, text=True, capture_output=True)
        if log:
            (output / log).write_text(result.stdout + result.stderr)
        if result.returncode:
            raise RuntimeError(f"Command failed ({result.returncode}): {command!r}\n"
                               f"{result.stdout}\n{result.stderr}")
        return result.stdout

    run(prefix + ["test", "-x", args.rewriter])
    run(prefix + ["taskset", "-c", str(args.cpu), "true"])
    if args.perf_bin:
        run(prefix + [args.perf_bin, "stat", "-e", "cycles", "--", "true"])
    output.mkdir()
    for name in companions + ["run_mpk_dispatch.py"]:
        shutil.copyfile(sources / name, output / name)
    manifest = {
        "arguments": vars(args) | {"output": str(output)},
        "source_sha256": {name: hashlib.sha256((sources / name).read_bytes()).hexdigest()
                          for name in companions + ["run_mpk_dispatch.py"]},
        "compiler_flags": "-O2 -Werror",
    }
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print("Building release runtime (without debug features)...", flush=True)
    run(prefix + [
        "env", "-u", "LIND_MPK_DEBUG", "-u", "LIND_DEBUG", "-u", "RUSTFLAGS",
        "-u", "CARGO_ENCODED_RUSTFLAGS", "make", "lind-boot",
        "LIND_BOOT_EXTRA_FEATURES=", "FDTABLES_IMPL=dashmaparray",
    ], "release-build.log")
    # Cargo prints its profile evidence on stderr, preserved in the build log.
    if "Finished `release` profile [optimized]" not in (output / "release-build.log").read_text():
        raise RuntimeError("Build log did not confirm an optimized release profile")
    run(prefix + ["cmp", "build/lind-boot", "src/lind-boot/target/release/lind-boot"])
    run(prefix + ["sh", "-c", "git rev-parse HEAD; git diff --stat; "
                  "gcc --version | head -1; rustc --version; "
                  "sha256sum glibc-2.35/build/libc.so.6"], "provenance.log")
    work = run(prefix + ["mktemp", "-d", "/tmp/lind-dispatch.XXXXXXXX"]).strip()
    guest = "tmp/" + PurePosixPath(work).name
    if not work.startswith("/tmp/lind-dispatch.") or "/" in PurePosixPath(work).name:
        raise RuntimeError(f"Unexpected mktemp path: {work!r}")
    try:
        for name in companions[:3]:
            run(["docker", "cp", str(sources / name), f"{args.container}:{work}/{name}"])
        for name in ["geteuid_dispatch", "geteuid_dispatch_grate"]:
            command = prefix + ["env", f"GSFORK_ELF_BIN={args.rewriter}",
                                "./scripts/lind_compile", "--target", "mpk"]
            if name.endswith("_grate"):
                command += ["--compile-grate"]
            command += ["--output-dir", guest, f"{work}/{name}.c", "--", "-O2", "-Werror"]
            run(command, f"{name}-build.log")
        run(prefix + ["gcc", "-O2", "-Wall", "-Wextra", "-Werror",
                      f"{work}/tsc_frequency.c", "-o", f"{work}/tsc_frequency"],
            "frequency-build.log")
        print("Running pinned measurements and process-level statistical validation...", flush=True)
        run([sys.executable, str(sources / "benchmark_dispatch.py"),
             "--container", args.container, "--repo", args.repo,
             "--work-dir", work, "--guest-dir", guest, "--cpu", str(args.cpu),
             "--runs", str(args.runs), "--output", str(output)], "analysis.log")
        results = json.loads((output / "results.json").read_text())
        scale = 1e9 / results["tsc_hz_used"]
        mean = results["net_mean_ticks"] * scale
        lo, hi = [v * scale for v in results["net_mean_ci95"]]
        print(f"Mean round trip: {mean:.2f} ns (95% CI {lo:.2f}-{hi:.2f} ns)")
        print(f"Raw single-call median/p95/p99: "
              f"{'/'.join(f'{v * scale:.1f}' for v in results['raw_single_ticks'].values())} ns")
        print(f"Complete results: {output}")
        if args.perf_bin:
            print("Profiling 100 million dispatches per perf invocation...", flush=True)
            workload = ["env", "-u", "LIND_MPK_DEBUG", "-u", "LIND_DEBUG",
                        "taskset", "-c", str(args.cpu), "./build/lind-boot",
                        f"{guest}/geteuid_dispatch_grate", f"{guest}/geteuid_dispatch",
                        "1000", "1000", "100000"]
            run(prefix + [args.perf_bin, "stat", "-o", f"{work}/perf-stat.txt",
                          "-e", "task-clock,cycles:u,instructions:u,branches:u,"
                          "branch-misses:u,context-switches,cpu-migrations,page-faults",
                          "--"] + workload, "perf-stat-benchmark.log")
            run(prefix + [args.perf_bin, "record", "-o", f"{work}/perf.data",
                          "-e", "cycles", "-F", "999", "--"] + workload,
                "perf-record-benchmark.log")
            run(prefix + [args.perf_bin, "report", "-i", f"{work}/perf.data",
                          "--stdio", "--no-children", "--percent-limit", "0.1"],
                "perf-report.txt")
            for symbol, name in [
                ("cage::cage::is_cage_dead", "cage-dead"),
                ("lind_boot::lind_mpk::trampoline::grate_callback_trampoline", "grate-context"),
                ("lind_boot::lind_mpk::trampoline::inner_grate_callback_trampoline_asm", "mpk-switch"),
            ]:
                run(prefix + [args.perf_bin, "annotate", "-i", f"{work}/perf.data",
                              "--stdio", "--symbol", symbol], f"perf-annotate-{name}.txt")
            run(prefix + [args.perf_bin, "stat", "-o", f"{work}/perf-syscalls.txt",
                          "-e", "syscalls:sys_enter_rt_sigprocmask,syscalls:sys_enter_arch_prctl,"
                          "syscalls:sys_enter_geteuid,syscalls:sys_enter_sched_yield",
                          "--"] + workload[:-3] + ["1000", "1000", "1000"],
                "perf-syscall-benchmark.log")
            for name in ["perf-stat.txt", "perf.data", "perf-syscalls.txt"]:
                run(["docker", "cp", f"{args.container}:{work}/{name}", str(output / name)])
            print(f"Perf counters, samples, reports, and annotations saved in {output}")
    finally:
        known = [f"{work}/{name}" for name in companions[:3]]
        known += [f"{work}/{name}" for name in
                  ["geteuid_dispatch", "geteuid_dispatch_grate", "tsc_frequency"]]
        known += [f"{args.repo}/lindfs/{guest}/{name}" for name in
                  ["geteuid_dispatch", "geteuid_dispatch_grate"]]
        if args.perf_bin:
            known += [f"{work}/{name}" for name in
                      ["perf-stat.txt", "perf.data", "perf.data.old", "perf-syscalls.txt"]]
        run(prefix + ["rm", "-f", "--"] + known)
        cleanup = subprocess.run(prefix + ["rmdir", "--", work, f"{args.repo}/lindfs/{guest}"],
                                 capture_output=True, text=True)
        if cleanup.returncode:
            print(f"Cleanup could not remove temporary directories: {cleanup.stderr}",
                  file=sys.stderr)


if __name__ == "__main__":
    main()
