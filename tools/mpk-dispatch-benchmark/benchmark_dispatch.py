import argparse
import csv
import datetime
import io
import json
import math
from pathlib import Path
import random
import statistics as st
import subprocess


def quantile(values, p):
    values = sorted(values)
    position = (len(values) - 1) * p
    low = int(position)
    return values[low] + (values[min(low + 1, len(values) - 1)] - values[low]) * (position - low)


def bootstrap(values, rng, statistic=st.mean):
    samples = [statistic(rng.choices(values, k=len(values))) for _ in range(10000)]
    return [quantile(samples, 0.025), quantile(samples, 0.975)]


def correlation(a, b):
    ma, mb = st.mean(a), st.mean(b)
    numerator = sum((x - ma) * (y - mb) for x, y in zip(a, b))
    denominator = math.sqrt(sum((x - ma) ** 2 for x in a) * sum((y - mb) ** 2 for y in b))
    return numerator / denominator if denominator else 0.0


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--container", default="cranky_keldysh")
    parser.add_argument("--cpu", type=int, default=8)
    parser.add_argument("--runs", type=int, default=40)
    parser.add_argument("--repo", default="/home/lind/lind-wasm")
    parser.add_argument("--work-dir", default="/tmp/lind-dispatch-bench-6191f5db")
    parser.add_argument("--guest-dir", default="tmp/dispatch-bench-6191f5db")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.runs < 20 or args.runs % 2:
        parser.error("--runs must be even and at least 20")
    args.output.mkdir(parents=True, exist_ok=True)
    prefix = ["docker", "exec", "-u", "root", "-w", args.repo, args.container]

    def execute(command):
        result = subprocess.run(prefix + command, capture_output=True, text=True, timeout=60)
        if result.returncode:
            raise RuntimeError(f"{command}: exit {result.returncode}\n{result.stdout}\n{result.stderr}")
        if result.stderr:
            raise RuntimeError(f"Unexpected runtime diagnostics: {result.stderr}")
        return result.stdout

    metadata = execute(["sh", "-c",
                        "uname -a; lscpu | grep -E 'Model name|Thread|Core|Socket'; "
                        "cat /sys/devices/system/clocksource/clocksource0/current_clocksource; "
                        "sha256sum build/lind-boot src/lind-boot/target/release/lind-boot; "
                        f"cat /sys/devices/system/cpu/cpu{args.cpu}/cpufreq/scaling_governor; "
                        f"cat /sys/devices/system/cpu/cpu{args.cpu}/topology/thread_siblings_list"])
    hashes = [line.split()[0] for line in metadata.splitlines() if "lind-boot" in line]
    if len(hashes) != 2 or hashes[0] != hashes[1]:
        raise RuntimeError("Installed runtime differs from the release artifact")
    frequency = json.loads(execute(["taskset", "-c", str(args.cpu),
                                   f"{args.work_dir}/tsc_frequency"]))
    calibrated = st.median(frequency["calibrated_hz"])
    hz = frequency["cpuid_hz"] or calibrated
    if abs(calibrated / hz - 1) > 0.001:
        raise RuntimeError("CPUID and measured TSC frequency disagree by more than 0.1%")
    rng = random.Random(6191)
    sizes = [100, 1000] * (args.runs // 2)
    rng.shuffle(sizes)
    summaries, pooled_single, pooled_empty = [], [], []
    for run, size in enumerate(sizes):
        command = ["env", "-u", "LIND_MPK_DEBUG", "-u", "LIND_DEBUG",
                   "taskset", "-c", str(args.cpu), "timeout", "30",
                   "./build/lind-boot",
                   f"{args.guest_dir}/geteuid_dispatch_grate",
                   f"{args.guest_dir}/geteuid_dispatch",
                   "10000", "1000", str(size)]
        output = execute(command)
        (args.output / f"run-{run:02d}.csv").write_text(output)
        rows = list(csv.DictReader(io.StringIO(output)))
        if len(rows) != 11000:
            raise RuntimeError(f"Run {run}: expected 11000 samples, got {len(rows)}")
        single, empty, batch, control = [], [], [], []
        for row in rows:
            dispatch, baseline, calls = [int(row[key]) for key in
                                         ("dispatch_ticks", "control_ticks", "calls")]
            if dispatch <= 0 or baseline <= 0:
                raise RuntimeError(f"Run {run}: invalid timestamp")
            if row["kind"] == "single" and calls == 1:
                single.append(dispatch)
                empty.append(baseline)
            elif row["kind"] == "batch" and calls == size:
                batch.append(dispatch / size)
                control.append(baseline / size)
            else:
                raise RuntimeError(f"Run {run}: invalid sample {row}")
        if len(single) != 10000 or len(batch) != 1000:
            raise RuntimeError(f"Run {run}: incorrect sample counts")
        net = [d - c for d, c in zip(batch, control)]
        summary = {
            "run": run, "batch_size": size,
            "command": " ".join(prefix + command),
            "batch_raw_mean": st.mean(batch), "control_mean": st.mean(control),
            "net_mean": st.mean(net), "net_median": st.median(net),
            "single_median": st.median(single), "single_p95": quantile(single, 0.95),
            "single_p99": quantile(single, 0.99), "empty_median": st.median(empty),
            "half_shift": st.median(net[500:]) - st.median(net[:500]),
            "batch_lag1": correlation(net[:-1], net[1:]),
        }
        summaries.append(summary)
        pooled_single.extend(single)
        pooled_empty.extend(empty)
        print(f"run {run + 1:02d}/{args.runs}: batch={size}, "
              f"net median={summary['net_median']:.2f}, mean={summary['net_mean']:.2f} ticks",
              flush=True)

    means = [s["net_mean"] for s in summaries]
    medians = [s["net_median"] for s in summaries]
    index = list(range(args.runs))
    trend = correlation(index, medians)
    permutations = [correlation(index, rng.sample(medians, len(medians))) for _ in range(10000)]
    trend_p = (1 + sum(abs(v) >= abs(trend) for v in permutations)) / 10001
    small = [s["net_median"] for s in summaries if s["batch_size"] == 100]
    large = [s["net_median"] for s in summaries if s["batch_size"] == 1000]
    relative_difference = 100 * (st.mean(large) / st.mean(small) - 1)
    differences = [100 * (st.mean(rng.choices(large, k=len(large))) /
                         st.mean(rng.choices(small, k=len(small))) - 1) for _ in range(10000)]
    difference_ci = [quantile(differences, 0.025), quantile(differences, 0.975)]
    positives = sum(v > 0 for v in means)
    k = min(positives, len(means) - positives)
    sign_p = min(1, 2 * sum(math.comb(len(means), i) for i in range(k + 1)) / 2 ** len(means))
    report = {
        "timestamp_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "container": args.container, "cpu": args.cpu, "metadata": metadata,
        "frequency": frequency, "tsc_hz_used": hz,
        "method": "cage-side LFENCE/RDTSC/LFENCE; silent constant-10 grate; "
                  "20000 warmup calls/run; alternating control order; random batch-size order; "
                  "no sample trimming; fresh processes are CI resampling units; "
                  "grate parent sleeps 10ms between WNOHANG checks to avoid Lind busy-wait",
        "runs": args.runs, "single_calls": len(pooled_single),
        "batched_calls": sum(1000 * size for size in sizes),
        "bootstrap_resamples": 10000, "seed": 6191,
        "net_mean_ticks": st.mean(means), "net_mean_ci95": bootstrap(means, rng),
        "net_typical_ticks": st.mean(medians), "net_typical_ci95": bootstrap(medians, rng),
        "run_median_cv_percent": 100 * st.stdev(medians) / st.mean(medians),
        "run_mean_cv_percent": 100 * st.stdev(means) / st.mean(means),
        "raw_batch_mean_ticks": st.mean([s["batch_raw_mean"] for s in summaries]),
        "local_control_mean_ticks": st.mean([s["control_mean"] for s in summaries]),
        "raw_single_ticks": {str(p): quantile(pooled_single, p) for p in (0.5, 0.95, 0.99)},
        "empty_interval_median_ticks": st.median(pooled_empty),
        "positive_dispatch_sign_test_p": sign_p,
        "run_order_correlation": trend, "run_order_permutation_p": trend_p,
        "within_run_half_shift_ticks": st.mean([s["half_shift"] for s in summaries]),
        "within_run_half_shift_ci95": bootstrap([s["half_shift"] for s in summaries], rng),
        "median_batch_lag1_correlation": st.median([s["batch_lag1"] for s in summaries]),
        "batch_size_relative_difference_percent": relative_difference,
        "batch_size_difference_ci95_percent": difference_ci,
        "batch_size_equivalent_within_5_percent": difference_ci[0] > -5 and difference_ci[1] < 5,
        "per_run": summaries,
        "limitations": [
            "Full libc geteuid + supervisor/3i + grate entry/return round trip, not one-way latency.",
            "TSC ticks are reference-clock ticks, not current core cycles.",
            "Local control subtraction estimates incremental trip cost; code paths are not identical.",
            "Fresh processes share a host; drift checks do not prove statistical independence.",
            "CPU affinity prevents migration but does not eliminate interrupts or SMT contention.",
            "Batched means amortize timestamps and do not estimate individual-call tail latency.",
        ],
    }
    (args.output / "results.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({key: value for key, value in report.items() if key != "per_run"}, indent=2))


if __name__ == "__main__":
    main()
