/// Call-time scheduling decision for a remotely-configured library function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchedulerDecision {
    Remote,
    Local,
}

/// Decides at call time whether a function that is *eligible* for remote dispatch
/// should actually be sent to the remote server or handled locally instead.
///
/// This sits after the static routing filter (which already ruled out functions
/// that are never remote) and acts as a dynamic, per-call policy. Typical future
/// implementations could consider server load, observed latency, cage priority,
/// or explicit user-supplied hints.
///
/// # Current behavior
/// Placeholder: always returns [`SchedulerDecision::Remote`].
///
/// TODO: implement load-based, latency-aware, or policy-driven scheduling.
pub struct Scheduler;

impl Scheduler {
    /// Return a scheduling decision for `symbol` at the point of the call.
    ///
    /// `symbol` — the name of the library function about to be dispatched.
    pub fn decide(_symbol: &str) -> SchedulerDecision {
        SchedulerDecision::Remote
    }
}
