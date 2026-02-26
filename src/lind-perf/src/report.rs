use crate::counter::Counter;
use crate::timers::{PrettyDuration, TimerKind};
use std::sync::atomic::Ordering;
use std::time::Duration;

/// Print a section header.
pub fn report_header(header: String) {
    let pad = "-";
    let total = 97 - header.len();
    let left = total / 2;
    let right = total - left;

    println!("\n{}{}{}", pad.repeat(left), header, pad.repeat(right),);
}

/// Print a report for a counter group.
///
/// The report is sorted by definition order, not by cost.
pub fn report(counters: &[&Counter]) {
    // Tunable constants
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

        // {:<UNIT_W$}
        rows.push(format!(
            "{:<NAME_W$} {:>CALLS_W$} {:>NUM_W$} {:>NUM_W$}",
            c.name, calls, cycles, avg,
        ));
    }

    if rows.len() == 0 {
        return;
    }

    eprintln!(
        "{:<NAME_W$} {:>CALLS_W$} {:>NUM_W$} {:>NUM_W$}",
        "name", "calls", "total", "avg",
    );

    eprintln!("{}", "-".repeat(NAME_W + CALLS_W + NUM_W * 2 + 3));

    for i in rows {
        eprintln!("{}", i);
    }

    println!("");
}
