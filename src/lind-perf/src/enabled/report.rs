use crate::{Counter, TimerKind};
use std::sync::atomic::Ordering;
use std::time::Duration;

/// Formats nanosecond totals for reports. Converts nanosecond input to larger units where
/// appropriate and truncates to 3 decimal points.
pub struct PrettyDuration(pub Duration);

impl std::fmt::Display for PrettyDuration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ns_f = self.0.as_nanos() as f64;

        let format = if ns_f < 1_000.0 {
            format!("{:.3}ns", ns_f)
        } else if ns_f < 1_000_000.0 {
            format!("{:.3}us", ns_f / 1_000.0)
        } else if ns_f < 1_000_000_000.0 {
            format!("{:.3}ms", ns_f / 1_000_000.0)
        } else {
            format!("{:.3}s", ns_f / 1_000_000_000.0)
        };

        write!(f, "{}", format)
    }
}

/// Print a section header.
pub fn report_header(header: String) {
    let pad = "-";
    let total = 97 - header.len();
    let left = total / 2;
    let right = total - left;

    println!("\n{}{}{}", pad.repeat(left), header, pad.repeat(right),);
}

/// Print a report for a counter group.
pub fn report(counters: &[&Counter]) {
    const NAME_W: usize = 60;
    const CALLS_W: usize = 10;
    const NUM_W: usize = 12;

    let mut rows: Vec<String> = Vec::new();

    for c in counters {
        let calls = c.calls.load(Ordering::Relaxed);
        if calls == 0 {
            continue;
        }

        let cycles = match c.timer_kind() {
            TimerKind::Rdtsc => format!("{:#?}", c.cycles.load(Ordering::Relaxed)),
            TimerKind::Clock => format!(
                "{}",
                PrettyDuration(Duration::from_nanos(c.cycles.load(Ordering::Relaxed)))
            ),
        };

        let avg = match c.timer_kind() {
            TimerKind::Rdtsc => format!("{:#?}", c.cycles.load(Ordering::Relaxed) / calls),
            TimerKind::Clock => format!(
                "{}",
                PrettyDuration(Duration::from_nanos(
                    c.cycles.load(Ordering::Relaxed) / calls
                ))
            ),
        };

        rows.push(format!(
            "{:<NAME_W$} {:>CALLS_W$} {:>NUM_W$} {:>NUM_W$}",
            c.name, calls, cycles, avg,
        ));
    }

    if rows.is_empty() {
        return;
    }

    eprintln!(
        "{:<NAME_W$} {:>CALLS_W$} {:>NUM_W$} {:>NUM_W$}",
        "name", "calls", "total", "avg",
    );
    eprintln!("{}", "-".repeat(NAME_W + CALLS_W + NUM_W * 2 + 3));

    for row in rows {
        eprintln!("{}", row);
    }

    println!("");
}
