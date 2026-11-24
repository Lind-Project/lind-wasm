//! Deterministic schedule for guest threads.
//!
//! When the schedule is enabled, one guest thread (the main thread of a cage
//! or a pthread) runs at a time. The thread that runs holds "the turn". It
//! keeps the turn until it blocks in a system call, yields, or ends, and the
//! turn then goes to the thread that has been ready for the longest.
//!
//! Every thread still has its own host thread; a thread without the turn
//! waits here. The host decides when a host thread makes progress, but not
//! which guest thread runs next: the ready queue is only changed by the
//! thread that holds the turn (when it starts a thread, wakes one, yields,
//! blocks or ends), so the order in which guest threads run is a function of
//! what the guests do.
//!
//! Three kinds of wait:
//! - [`yield_now`]: the thread stays ready and goes to the back of the queue
//!   (sched_yield, nanosleep).
//! - [`wait`]: the thread leaves the queue until [`wake`] names its channel
//!   (waitpid, futex wait without a timeout).
//! - [`wait_timed`]: the thread waits on a channel for one round of the ready
//!   queue. There is no real time here, so a timeout is "every other ready
//!   thread has had a turn and nobody woke me".
//!
//! A thread that was never given a ticket (a runtime helper thread, or any
//! thread while the schedule is disabled) is not scheduled: [`active`] is
//! false for it and callers keep their ordinary blocking path.

use std::cell::Cell;
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

/// What a blocked thread waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Chan {
    /// An event for a cage: a child exited, a signal arrived, the cage is
    /// being killed. Keyed by the cage id of the waiter.
    Cage(u64),
    /// A futex word, keyed by its host address.
    Futex(u64),
    /// A free worker of a grate, keyed by the grate's cage id.
    Worker(u64),
}

/// Why a wait returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wake {
    /// [`wake`] named the channel.
    Woken,
    /// A timed wait saw one round pass without a wake.
    TimedOut,
    /// [`interrupt_cage`] ended the wait.
    Interrupted,
}

struct Slot {
    cage: u64,
    cv: Arc<Condvar>,
    /// The channel this thread is blocked on, if any.
    wait: Option<Chan>,
    in_ready: bool,
    interrupted: bool,
    /// See [`set_unblocked_signal`].
    unblocked_signal: bool,
}

struct State {
    /// The thread that holds the turn.
    running: Option<u64>,
    /// Threads that can run, oldest first.
    ready: VecDeque<u64>,
    /// Threads blocked on a channel, oldest first.
    blocked: BTreeMap<Chan, VecDeque<u64>>,
    slots: BTreeMap<u64, Slot>,
    next_id: u64,
    /// Number of times the turn was handed to a thread.
    switches: u64,
    /// Number of times no thread was ready when the turn was given up.
    stalls: u64,
    /// Number of yields that found no other thread ready.
    lone_yields: u64,
}

impl State {
    const fn new() -> Self {
        State {
            running: None,
            ready: VecDeque::new(),
            blocked: BTreeMap::new(),
            slots: BTreeMap::new(),
            next_id: 1,
            switches: 0,
            stalls: 0,
            lone_yields: 0,
        }
    }

    /// Give the turn to the oldest ready thread if nobody holds it.
    fn dispatch(&mut self) {
        if self.running.is_some() {
            return;
        }
        match self.ready.pop_front() {
            Some(id) => {
                let slot = self
                    .slots
                    .get_mut(&id)
                    .expect("sched: ready thread has no slot");
                slot.in_ready = false;
                self.running = Some(id);
                self.switches += 1;
                slot.cv.notify_all();
            }
            None => {
                if self.slots.values().any(|s| s.wait.is_some()) {
                    self.stalls += 1;
                    if self.stalls <= STALL_REPORTS {
                        report(&format!(
                            "[sched] no thread is ready: {}\n",
                            self.describe_blocked()
                        ));
                    }
                }
            }
        }
    }

    fn describe_blocked(&self) -> String {
        let chans: Vec<String> = self
            .blocked
            .iter()
            .map(|(c, q)| {
                let cages: Vec<String> = q
                    .iter()
                    .map(|id| self.slots.get(id).map_or(0, |s| s.cage).to_string())
                    .collect();
                format!("{:?}<-cage {}", c, cages.join(","))
            })
            .collect();
        format!("{} threads, blocked: {}", self.slots.len(), chans.join(" "))
    }

    fn make_ready(&mut self, id: u64) {
        if let Some(slot) = self.slots.get_mut(&id) {
            if !slot.in_ready {
                slot.in_ready = true;
                self.ready.push_back(id);
            }
        }
    }

    /// Take `id` off the channel it is blocked on.
    fn unblock(&mut self, id: u64) {
        let chan = match self.slots.get_mut(&id).and_then(|s| s.wait.take()) {
            Some(c) => c,
            None => return,
        };
        if let Some(q) = self.blocked.get_mut(&chan) {
            q.retain(|x| *x != id);
            if q.is_empty() {
                self.blocked.remove(&chan);
            }
        }
    }

    fn block_on(&mut self, id: u64, chan: Chan) {
        self.unblock(id);
        if let Some(slot) = self.slots.get_mut(&id) {
            slot.wait = Some(chan);
            self.blocked.entry(chan).or_default().push_back(id);
        }
    }
}

/// How many "no thread is ready" lines are written to stderr.
const STALL_REPORTS: u64 = 8;
/// A thread that yields this many times (and at every doubling after it)
/// while no other thread is ready is reported: it polls for something that
/// no thread can bring about.
const LONE_YIELD_REPORT: u64 = 1 << 21;

/// One write per line: several threads may report at once.
fn report(line: &str) {
    unsafe {
        libc::write(2, line.as_ptr() as *const libc::c_void, line.len());
    }
}

static ENABLED: AtomicBool = AtomicBool::new(false);
static STATE: Mutex<State> = Mutex::new(State::new());

thread_local! {
    static CURRENT: Cell<Option<u64>> = const { Cell::new(None) };
}

fn lock() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Turn the schedule on. Call it before the first guest thread starts.
pub fn enable() {
    ENABLED.store(true, Ordering::SeqCst);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

fn current() -> Option<u64> {
    if !enabled() {
        return None;
    }
    CURRENT.with(|c| c.get())
}

/// True when the calling thread is scheduled here, i.e. it holds the turn.
pub fn active() -> bool {
    current().is_some()
}

/// The cage the calling thread was registered for, when it is scheduled here.
///
/// Inside a grate call this is still the cage that made the call, because a
/// grate runs on its caller's thread.
pub fn current_cage() -> Option<u64> {
    let me = current()?;
    lock().slots.get(&me).map(|s| s.cage)
}

/// A place in the ready queue for a thread that has not started yet.
pub struct Ticket(Option<u64>);

/// The right to run. Dropping it ends the thread's part in the schedule and
/// hands the turn to the next ready thread.
pub struct Turn(Option<u64>);

/// Put a new thread at the back of the ready queue.
///
/// The thread that creates a guest thread calls this (it holds the turn, so
/// the position is decided by the guests); the new thread calls
/// [`Ticket::start`] before it runs guest code.
pub fn register(cage: u64) -> Ticket {
    if !enabled() {
        return Ticket(None);
    }
    let mut st = lock();
    let id = st.next_id;
    st.next_id += 1;
    st.slots.insert(
        id,
        Slot {
            cage,
            cv: Arc::new(Condvar::new()),
            wait: None,
            in_ready: false,
            interrupted: false,
            unblocked_signal: false,
        },
    );
    st.make_ready(id);
    st.dispatch();
    Ticket(Some(id))
}

impl Ticket {
    /// Wait for the turn. Returns when this thread is the one that runs.
    pub fn start(mut self) -> Turn {
        let id = match self.0.take() {
            Some(id) => id,
            None => return Turn(None),
        };
        let mut st = lock();
        let cv = st.slots[&id].cv.clone();
        while st.running != Some(id) {
            st = cv.wait(st).unwrap_or_else(|e| e.into_inner());
        }
        drop(st);
        CURRENT.with(|c| c.set(Some(id)));
        Turn(Some(id))
    }
}

impl Drop for Ticket {
    /// A ticket that is never started (the thread could not be created)
    /// must not stay in the queue.
    fn drop(&mut self) {
        if let Some(id) = self.0.take() {
            remove(id);
        }
    }
}

impl Drop for Turn {
    fn drop(&mut self) {
        if let Some(id) = self.0.take() {
            CURRENT.with(|c| c.set(None));
            remove(id);
        }
    }
}

fn remove(id: u64) {
    let mut st = lock();
    st.unblock(id);
    st.ready.retain(|x| *x != id);
    st.slots.remove(&id);
    if st.running == Some(id) {
        st.running = None;
    }
    st.dispatch();
}

/// Give up the turn and wait until it comes back.
fn give_up(mut st: MutexGuard<'static, State>, me: u64) -> MutexGuard<'static, State> {
    let cv = st.slots[&me].cv.clone();
    debug_assert_eq!(st.running, Some(me), "sched: a thread without the turn ran");
    st.running = None;
    st.dispatch();
    while st.running != Some(me) {
        st = cv.wait(st).unwrap_or_else(|e| e.into_inner());
    }
    st
}

/// Let every other ready thread run before this one continues.
pub fn yield_now() {
    let me = match current() {
        Some(me) => me,
        None => {
            std::thread::yield_now();
            return;
        }
    };
    let mut st = lock();
    if st.ready.is_empty() {
        st.lone_yields += 1;
        if st.lone_yields >= LONE_YIELD_REPORT && st.lone_yields.is_power_of_two() {
            let cage = st.slots.get(&me).map_or(0, |s| s.cage);
            report(&format!(
                "[sched] a thread of cage {} has yielded {} times with no other thread ready: {}\n",
                cage,
                st.lone_yields,
                st.describe_blocked()
            ));
        }
        return;
    }
    st.make_ready(me);
    drop(give_up(st, me));
}

/// Record (or clear) that a signal became deliverable when the calling
/// thread changed its signal mask.
///
/// A program that needs "unblock a signal and wait" to be one step calls
/// pselect; where pselect is sigprocmask + select + sigprocmask, a signal
/// that was pending is handled right after the first sigprocmask and the
/// select then waits for a signal that has already come. Under the
/// deterministic schedule the runtime closes that gap: the next sleep of
/// this thread ends with EINTR (see [`take_unblocked_signal`]), as if the
/// signal had arrived in the wait. Every change of the mask sets or clears
/// the record, so it lasts from one sigprocmask to the next.
pub fn set_unblocked_signal(value: bool) {
    if let Some(me) = current() {
        if let Some(slot) = lock().slots.get_mut(&me) {
            slot.unblocked_signal = value;
        }
    }
}

/// True once after [`set_unblocked_signal`]`(true)`.
pub fn take_unblocked_signal() -> bool {
    match current() {
        Some(me) => lock()
            .slots
            .get_mut(&me)
            .is_some_and(|slot| std::mem::take(&mut slot.unblocked_signal)),
        None => false,
    }
}

/// First half of [`wait`]: join the channel but keep running.
///
/// A caller that checks its condition under a lock of its own calls this
/// while it still holds that lock, releases the lock and then calls
/// [`park`]. A [`wake`] that arrives in between is not lost: `park` then
/// behaves like a yield.
pub fn prepare(chan: Chan) {
    if let Some(me) = current() {
        lock().block_on(me, chan);
    }
}

/// Undo [`prepare`]: the caller found that it does not have to wait.
pub fn cancel() {
    if let Some(me) = current() {
        let mut st = lock();
        st.unblock(me);
        if let Some(slot) = st.slots.get_mut(&me) {
            slot.interrupted = false;
        }
    }
}

/// Second half of [`wait`]: give up the turn until the channel is woken.
pub fn park() -> Wake {
    let me = match current() {
        Some(me) => me,
        None => return Wake::Woken,
    };
    let st = lock();
    let mut st = give_up(st, me);
    let slot = st.slots.get_mut(&me).expect("sched: no slot");
    if std::mem::take(&mut slot.interrupted) {
        Wake::Interrupted
    } else {
        Wake::Woken
    }
}

/// Block until [`wake`] names `chan`.
pub fn wait(chan: Chan) -> Wake {
    prepare(chan);
    park()
}

/// Wait on `chan` for one round of the ready queue.
pub fn wait_timed(chan: Chan) -> Wake {
    let me = match current() {
        Some(me) => me,
        None => return Wake::TimedOut,
    };
    let mut st = lock();
    if st.ready.is_empty() {
        // Nobody else can run, so nobody can wake this thread.
        return Wake::TimedOut;
    }
    st.block_on(me, chan);
    st.make_ready(me);
    let mut st = give_up(st, me);
    let slot = st.slots.get_mut(&me).expect("sched: no slot");
    let interrupted = std::mem::take(&mut slot.interrupted);
    let still_waiting = slot.wait.is_some();
    if still_waiting {
        st.unblock(me);
    }
    if interrupted {
        Wake::Interrupted
    } else if still_waiting {
        Wake::TimedOut
    } else {
        Wake::Woken
    }
}

/// Make up to `max` threads blocked on `chan` ready, oldest first.
/// Returns how many there were.
pub fn wake(chan: Chan, max: usize) -> usize {
    if !enabled() {
        return 0;
    }
    let mut st = lock();
    let mut n = 0;
    while n < max {
        let id = match st.blocked.get_mut(&chan).and_then(|q| q.pop_front()) {
            Some(id) => id,
            None => break,
        };
        if let Some(slot) = st.slots.get_mut(&id) {
            slot.wait = None;
        }
        st.make_ready(id);
        n += 1;
    }
    if st.blocked.get(&chan).is_some_and(|q| q.is_empty()) {
        st.blocked.remove(&chan);
    }
    // Only matters when a thread outside the schedule wakes someone while
    // nobody holds the turn.
    st.dispatch();
    n
}

/// End the wait of every blocked thread of `cage` with [`Wake::Interrupted`].
pub fn interrupt_cage(cage: u64) {
    if !enabled() {
        return;
    }
    let mut st = lock();
    let ids: Vec<u64> = st
        .slots
        .iter()
        .filter(|(_, s)| s.cage == cage && s.wait.is_some())
        .map(|(id, _)| *id)
        .collect();
    for id in ids {
        st.unblock(id);
        if let Some(slot) = st.slots.get_mut(&id) {
            slot.interrupted = true;
        }
        st.make_ready(id);
    }
    st.dispatch();
}

/// (turn hand-overs, times no thread was ready, threads known now).
pub fn stats() -> (u64, u64, usize) {
    let st = lock();
    (st.switches, st.stalls, st.slots.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;
    use std::thread;
    use std::time::Duration;

    /// The schedule is one global: run one test at a time.
    static SERIAL: Mutex<()> = Mutex::new(());

    fn serial() -> MutexGuard<'static, ()> {
        let g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        enable();
        assert_eq!(stats().2, 0, "a previous test left threads behind");
        g
    }

    /// A cheap deterministic generator for host-side delays.
    fn jitter(seed: &AtomicU64) {
        let mut x = seed.load(Ordering::Relaxed);
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        seed.store(x, Ordering::Relaxed);
        match x % 4 {
            0 => {}
            1 => thread::yield_now(),
            _ => thread::sleep(Duration::from_micros(x % 300)),
        }
    }

    /// `n` threads that each log and yield `rounds` times. The host-side
    /// delays differ with `seed`; the log must not.
    fn round_robin(n: u64, rounds: u64, seed: u64) -> Vec<(u64, u64)> {
        let log = Arc::new(Mutex::new(Vec::new()));
        let seed = Arc::new(AtomicU64::new(seed | 1));
        let main = register(0).start();
        let mut handles = Vec::new();
        for t in 0..n {
            let ticket = register(t);
            let log = log.clone();
            let seed = seed.clone();
            handles.push(thread::spawn(move || {
                jitter(&seed);
                let _turn = ticket.start();
                for r in 0..rounds {
                    log.lock().unwrap().push((t, r));
                    jitter(&seed);
                    yield_now();
                }
            }));
        }
        drop(main);
        for h in handles {
            h.join().unwrap();
        }
        Arc::try_unwrap(log).unwrap().into_inner().unwrap()
    }

    #[test]
    fn order_does_not_depend_on_host_timing() {
        let _g = serial();
        let first = round_robin(6, 20, 1);
        let expected: Vec<(u64, u64)> = (0..20).flat_map(|r| (0..6).map(move |t| (t, r))).collect();
        assert_eq!(first, expected);
        for seed in 2..12 {
            assert_eq!(round_robin(6, 20, seed * 7919), first, "seed {seed}");
        }
    }

    #[test]
    fn parent_runs_until_it_blocks() {
        let _g = serial();
        let log = Arc::new(Mutex::new(Vec::new()));
        let main = register(1).start();
        let ticket = register(2);
        let l = log.clone();
        let child = thread::spawn(move || {
            let _turn = ticket.start();
            l.lock().unwrap().push("child");
            assert_eq!(wake(Chan::Cage(1), usize::MAX), 1);
            l.lock().unwrap().push("child end");
        });
        // The child is already runnable for the host, but has no turn.
        thread::sleep(Duration::from_millis(20));
        log.lock().unwrap().push("parent");
        assert_eq!(wait(Chan::Cage(1)), Wake::Woken);
        log.lock().unwrap().push("parent woken");
        drop(main);
        child.join().unwrap();
        assert_eq!(
            *log.lock().unwrap(),
            vec!["parent", "child", "child end", "parent woken"]
        );
    }

    #[test]
    fn wake_is_first_come_first_served() {
        let _g = serial();
        let log = Arc::new(Mutex::new(Vec::new()));
        let seed = Arc::new(AtomicU64::new(99));
        let main = register(0).start();
        let mut handles = Vec::new();
        for t in 1..=5u64 {
            let ticket = register(t);
            let log = log.clone();
            let seed = seed.clone();
            handles.push(thread::spawn(move || {
                jitter(&seed);
                let _turn = ticket.start();
                assert_eq!(wait(Chan::Futex(0x1000)), Wake::Woken);
                log.lock().unwrap().push(t);
            }));
        }
        // Let all five block, in registration order.
        yield_now();
        assert_eq!(wake(Chan::Futex(0x1000), 2), 2);
        yield_now();
        assert_eq!(*log.lock().unwrap(), vec![1, 2]);
        assert_eq!(wake(Chan::Futex(0x1000), usize::MAX), 3);
        assert_eq!(wake(Chan::Futex(0x1000), usize::MAX), 0);
        drop(main);
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(*log.lock().unwrap(), vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn timed_wait_lasts_one_round() {
        let _g = serial();
        let main = register(0).start();
        // Alone: nobody can wake us.
        assert_eq!(wait_timed(Chan::Futex(8)), Wake::TimedOut);

        let woke = Arc::new(AtomicBool::new(false));
        let t1 = register(1);
        let h1 = thread::spawn(move || {
            let _turn = t1.start();
            // First round: do nothing. Second round: wake the waiter.
            yield_now();
            wake(Chan::Futex(8), 1);
        });
        assert_eq!(wait_timed(Chan::Futex(8)), Wake::TimedOut);
        assert_eq!(wait_timed(Chan::Futex(8)), Wake::Woken);
        woke.store(true, Ordering::SeqCst);
        drop(main);
        h1.join().unwrap();
        assert_eq!(stats().2, 0);
    }

    #[test]
    fn prepare_then_wake_then_park_does_not_block() {
        let _g = serial();
        let main = register(0).start();
        prepare(Chan::Cage(7));
        // A thread outside the schedule (a timer) wakes us before we park.
        thread::spawn(|| {
            wake(Chan::Cage(7), usize::MAX);
        })
        .join()
        .unwrap();
        assert_eq!(park(), Wake::Woken);
        drop(main);
    }

    #[test]
    fn interrupt_ends_the_waits_of_one_cage() {
        let _g = serial();
        let main = register(0).start();
        let mut handles = Vec::new();
        let results = Arc::new(Mutex::new(Vec::new()));
        for cage in [5u64, 6, 5] {
            let ticket = register(cage);
            let results = results.clone();
            handles.push(thread::spawn(move || {
                let _turn = ticket.start();
                let w = wait(Chan::Futex(0x2000 + cage));
                results.lock().unwrap().push((cage, w));
            }));
        }
        yield_now();
        interrupt_cage(5);
        yield_now();
        assert_eq!(
            *results.lock().unwrap(),
            vec![(5, Wake::Interrupted), (5, Wake::Interrupted)]
        );
        assert_eq!(wake(Chan::Futex(0x2006), 1), 1);
        drop(main);
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(results.lock().unwrap().len(), 3);
    }

    #[test]
    fn unscheduled_threads_are_left_alone() {
        let _g = serial();
        assert!(!active());
        assert_eq!(current_cage(), None);
        assert!(!take_unblocked_signal());
        {
            let turn = register(42).start();
            assert_eq!(current_cage(), Some(42));
            assert!(!take_unblocked_signal());
            set_unblocked_signal(true);
            assert!(take_unblocked_signal());
            assert!(!take_unblocked_signal());
            drop(turn);
        }
        yield_now();
        assert_eq!(park(), Wake::Woken);
        assert_eq!(wait_timed(Chan::Cage(1)), Wake::TimedOut);
        let unused = register(3);
        drop(unused);
        assert_eq!(stats().2, 0);
    }
}
