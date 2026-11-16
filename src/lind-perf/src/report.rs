use crate::counter::{Counter, Unit};
use crate::timers::{TimerKind, timer_kind};

const NAME_W: usize = 52;
const CALLS_W: usize = 10;
const NUM_W: usize = 13;
const LINE_W: usize = NAME_W + CALLS_W + NUM_W * 3 + 4;

fn unit_of(c: &Counter) -> Unit {
    // In clock mode, timer counters are nanoseconds too, so they compare with `Nanos` ones.
    match (c.unit, timer_kind()) {
        (Unit::Timer, TimerKind::Rdtsc) => Unit::Timer,
        _ => Unit::Nanos,
    }
}

fn fmt_value(v: f64, unit: Unit) -> String {
    match unit {
        Unit::Timer => format!("{:.0} cyc", v),
        Unit::Nanos => {
            if v.abs() < 1_000.0 {
                format!("{:.1} ns", v)
            } else if v.abs() < 1_000_000.0 {
                format!("{:.3} us", v / 1_000.0)
            } else if v.abs() < 1_000_000_000.0 {
                format!("{:.3} ms", v / 1_000_000.0)
            } else {
                format!("{:.3} s", v / 1_000_000_000.0)
            }
        }
    }
}

fn layer(name: &str) -> &str {
    name.split("::").next().unwrap_or(name)
}

fn header(title: &str) {
    let title = format!(" {} ", title);
    let pad = LINE_W.saturating_sub(title.len());
    println!("\n{}{}{}", "-".repeat(pad / 2), title, "-".repeat(pad - pad / 2));
}

/// Print every counter that recorded something, grouped by layer (the name prefix before
/// `::`), in the order given.
pub fn print_report(counters: &[&Counter]) {
    // Timer overhead differs slightly per run; show the range that was subtracted.
    let overheads: Vec<u64> = counters
        .iter()
        .filter(|c| c.calls() > 0 && c.unit == Unit::Timer)
        .map(|c| c.overhead())
        .collect();
    if let (Some(lo), Some(hi)) = (overheads.iter().min(), overheads.iter().max()) {
        let unit = match timer_kind() {
            TimerKind::Rdtsc => Unit::Timer,
            TimerKind::Clock => Unit::Nanos,
        };
        if lo == hi {
            println!(
                "\ntimer overhead per measurement: {} (subtracted in net/call and DERIVED)",
                fmt_value(*lo as f64, unit)
            );
        } else {
            println!(
                "\ntimer overhead per measurement: {} .. {} (measured in each run; subtracted in net/call and DERIVED)",
                fmt_value(*lo as f64, unit),
                fmt_value(*hi as f64, unit)
            );
        }
    }

    let mut current_layer = "";
    for c in counters {
        if c.calls() == 0 {
            continue;
        }
        if layer(c.name) != current_layer {
            current_layer = layer(c.name);
            header(&current_layer.to_uppercase());
            println!(
                "{:<NAME_W$} {:>CALLS_W$} {:>NUM_W$} {:>NUM_W$} {:>NUM_W$}",
                "name", "calls", "total", "avg/call", "net/call"
            );
            println!("{}", "-".repeat(LINE_W));
        }
        let unit = unit_of(c);
        println!(
            "{:<NAME_W$} {:>CALLS_W$} {:>NUM_W$} {:>NUM_W$} {:>NUM_W$}",
            c.name,
            c.calls(),
            fmt_value(c.total() as f64, unit),
            fmt_value(c.avg().unwrap_or(0.0), unit),
            fmt_value(c.net_avg().unwrap_or(0.0), unit),
        );
    }
}

/// Print a per-syscall breakdown: one row per (syscall, origin) seen by `entry`, sorted by the
/// total time spent in `entry`, showing the net cost per call in each of `columns`.
///
/// `entry` should be the outermost host-side counter (the syscall entry), so its per-slot
/// totals cover every timed syscall. Each column's values come from that counter's own run.
pub fn print_syscall_table(entry: &Counter, columns: &[(&str, &Counter)], top: usize) {
    let mut rows: Vec<(usize, u64, u64)> = entry.nonzero_slots().collect();
    if rows.is_empty() {
        return;
    }
    rows.sort_by(|a, b| b.1.cmp(&a.1));
    let grand: u64 = rows.iter().map(|r| r.1).sum();
    let unit = unit_of(entry);

    const SYS_W: usize = 26;
    const ORIG_W: usize = 6;
    const COL_W: usize = 11;
    let line_w = SYS_W + ORIG_W + CALLS_W + COL_W + 7 + (COL_W + 1) * (columns.len() + 1) + 4;

    let title = " PER SYSCALL (net/call; total = time in the syscall entry) ";
    let pad = line_w.saturating_sub(title.len());
    println!("\n{}{}{}", "-".repeat(pad / 2), title, "-".repeat(pad - pad / 2));
    print!(
        "{:<SYS_W$} {:<ORIG_W$} {:>CALLS_W$} {:>COL_W$} {:>6} {:>COL_W$}",
        "syscall", "origin", "calls", "total", "share", "entry"
    );
    for (label, _) in columns {
        print!(" {:>COL_W$}", label);
    }
    println!();
    println!("{}", "-".repeat(line_w));

    for (slot, total, calls) in rows.iter().take(top) {
        let (origin, num) = crate::bench::slot_info(*slot);
        let name = match num {
            Some(n) => format!("{} ({})", crate::names::syscall_name(n), n),
            None => "other".to_string(),
        };
        let share = if grand > 0 {
            100.0 * *total as f64 / grand as f64
        } else {
            0.0
        };
        print!(
            "{:<SYS_W$} {:<ORIG_W$} {:>CALLS_W$} {:>COL_W$} {:>5.1}% {:>COL_W$}",
            name,
            origin.label(),
            calls,
            fmt_value(*total as f64, unit),
            share,
            fmt_value(entry.slot_net_avg(*slot).unwrap_or(0.0), unit),
        );
        for (_, c) in columns {
            match c.slot_net_avg(*slot) {
                Some(v) => print!(" {:>COL_W$}", fmt_value(v, unit_of(c))),
                None => print!(" {:>COL_W$}", "-"),
            }
        }
        println!();
    }
    if rows.len() > top {
        let rest: u64 = rows.iter().skip(top).map(|r| r.1).sum();
        println!(
            "... {} more rows ({:.1}% of the time); raise --perf-top to see them",
            rows.len() - top,
            100.0 * rest as f64 / grand.max(1) as f64
        );
    }
    println!(
        "origin: cage = program's own call, grate = grate's own call, fwd = made by a grate while \
         servicing another call"
    );
}

/// A cost derived from nested timers: `outer` net avg minus the sum of the `inner` net avgs.
pub struct Derived {
    pub label: &'static str,
    pub outer: &'static Counter,
    pub inner: &'static [&'static Counter],
}

/// Print each derived cost whose counters all recorded something in the same unit. Lines
/// that don't apply to this benchmark (e.g. grate-path lines in a direct run) are skipped.
pub fn print_derived(rows: &[Derived]) {
    let mut lines = Vec::new();
    for d in rows {
        let Some(outer) = d.outer.net_avg() else { continue };
        let unit = unit_of(d.outer);
        let mut sum = 0.0;
        let mut ok = true;
        for c in d.inner {
            match c.net_avg() {
                Some(v) if unit_of(c) == unit => sum += v,
                _ => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            lines.push(format!(
                "{:<w$} {:>NUM_W$}",
                d.label,
                fmt_value(outer - sum, unit),
                w = LINE_W - NUM_W - 1
            ));
        }
    }
    if lines.is_empty() {
        return;
    }
    header("DERIVED (net/call: outer minus nested timers)");
    for l in lines {
        println!("{}", l);
    }
    println!();
}
