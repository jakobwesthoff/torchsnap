// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Hiding the Linux launcher after a mapped shrink
//
// On Linux the launcher frees its window buffers only when it
// shrinks to 1x1 while it is still mapped, so the hide waits a
// short, fixed time after the shrink. During that wait the
// launcher is mapped but already dismissed. This unit holds the
// decisions for that wait (is a hide pending, may it still fire)
// without GTK and without real time, so tests drive it with a
// fake timer.
// =========================================================

use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How long the window stays mapped at 1x1 before it is hidden. It
/// covers the release of the window buffers after a mapped shrink,
/// which was measured up to about 120 ms. See
/// `docs/research/linux-launcher-memory.md`, section "When the mapped
/// shrink has taken effect".
pub(crate) const MAPPED_SHRINK_WAIT: Duration = Duration::from_millis(150);

/// Proof that a hide was started by `HideGate::begin`.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) struct Ticket(u64);

#[derive(Debug, Default)]
struct GateState {
    generation: u64,
    pending: bool,
}

/// Whether a delayed hide is pending, and whether a timer that
/// fires may still carry it out.
#[derive(Debug, Default)]
pub(crate) struct HideGate {
    state: Mutex<GateState>,
}

impl HideGate {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, GateState> {
        self.state.lock().expect("hide gate lock not poisoned")
    }

    /// Start a hide. `None` when one is already pending.
    pub(crate) fn begin(&self) -> Option<Ticket> {
        let mut state = self.lock();
        if state.pending {
            return None;
        }
        state.pending = true;
        Some(Ticket(state.generation))
    }

    /// Drop a pending hide. Returns whether one was pending. The
    /// generation moves on, so the timer of the dropped hide stays
    /// stale even when a new hide begins before it fires.
    pub(crate) fn cancel(&self) -> bool {
        let mut state = self.lock();
        let was_pending = state.pending;
        state.pending = false;
        state.generation += 1;
        was_pending
    }

    /// Claim the hide for a firing timer. True only for the ticket of
    /// the hide that is still pending.
    pub(crate) fn fire(&self, ticket: Ticket) -> bool {
        let mut state = self.lock();
        if state.pending && state.generation == ticket.0 {
            state.pending = false;
            true
        } else {
            false
        }
    }

    pub(crate) fn is_pending(&self) -> bool {
        self.lock().pending
    }
}

/// The delayed part of a hide.
pub(crate) type HideJob = Box<dyn FnOnce() + Send>;

/// Runs a job after a delay.
pub(crate) trait HideTimer {
    fn after(&self, delay: Duration, job: HideJob);
}

/// Shrink now, hide after `MAPPED_SHRINK_WAIT` unless a show cancelled
/// the hide in between.
pub(crate) fn hide_after_mapped_shrink(
    gate: &Arc<HideGate>,
    timer: &impl HideTimer,
    shrink: impl FnOnce(),
    hide: impl FnOnce() + Send + 'static,
) {
    let Some(ticket) = gate.begin() else {
        return;
    };
    shrink();
    let gate = Arc::clone(gate);
    timer.after(
        MAPPED_SHRINK_WAIT,
        Box::new(move || {
            if gate.fire(ticket) {
                hide();
            }
        }),
    );
}

/// Whether a show or toggle should treat the launcher as open. A
/// pending hide has already dismissed it for the user.
pub(crate) fn counts_as_visible(is_visible: bool, hide_pending: bool) -> bool {
    is_visible && !hide_pending
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// Keeps the jobs until a test runs them, so nothing sleeps.
    #[derive(Default)]
    struct FakeTimer {
        jobs: Mutex<Vec<(Duration, HideJob)>>,
    }

    impl HideTimer for FakeTimer {
        fn after(&self, delay: Duration, job: HideJob) {
            self.jobs.lock().unwrap().push((delay, job));
        }
    }

    impl FakeTimer {
        fn armed(&self) -> usize {
            self.jobs.lock().unwrap().len()
        }

        fn delay_of(&self, index: usize) -> Duration {
            self.jobs.lock().unwrap()[index].0
        }

        fn run(&self, index: usize) {
            let job = self.jobs.lock().unwrap().remove(index).1;
            job();
        }
    }

    type Log = Arc<Mutex<Vec<&'static str>>>;

    fn dismiss(gate: &Arc<HideGate>, timer: &FakeTimer, log: &Log) {
        let on_shrink = Arc::clone(log);
        let on_hide = Arc::clone(log);
        hide_after_mapped_shrink(
            gate,
            timer,
            move || on_shrink.lock().unwrap().push("shrink"),
            move || on_hide.lock().unwrap().push("hide"),
        );
    }

    fn logged(log: &Log) -> Vec<&'static str> {
        log.lock().unwrap().clone()
    }

    #[test]
    fn begin_twice_gives_one_ticket() {
        let gate = HideGate::new();
        assert!(gate.begin().is_some());
        assert!(gate.begin().is_none());
        assert!(gate.is_pending());
    }

    #[test]
    fn cancel_reports_whether_a_hide_was_pending() {
        let gate = HideGate::new();
        assert!(!gate.cancel());
        gate.begin();
        assert!(gate.cancel());
        assert!(!gate.is_pending());
        assert!(!gate.cancel());
    }

    #[test]
    fn a_ticket_fires_once() {
        let gate = HideGate::new();
        let ticket = gate.begin().unwrap();
        assert!(gate.fire(ticket));
        assert!(!gate.is_pending());
        assert!(!gate.fire(ticket));
    }

    #[test]
    fn a_cancelled_ticket_does_not_fire() {
        let gate = HideGate::new();
        let ticket = gate.begin().unwrap();
        gate.cancel();
        assert!(!gate.fire(ticket));
    }

    #[test]
    fn a_ticket_of_a_cancelled_hide_stays_stale_after_a_new_hide_began() {
        let gate = HideGate::new();
        let old = gate.begin().unwrap();
        gate.cancel();
        let new = gate.begin().unwrap();
        assert!(!gate.fire(old));
        assert!(gate.is_pending());
        assert!(gate.fire(new));
    }

    #[test]
    fn the_shrink_runs_at_once_and_the_hide_waits_for_the_timer() {
        let gate = Arc::new(HideGate::new());
        let timer = FakeTimer::default();
        let log = Log::default();
        dismiss(&gate, &timer, &log);
        assert_eq!(logged(&log), ["shrink"]);
        assert_eq!(timer.armed(), 1);
        timer.run(0);
        assert_eq!(logged(&log), ["shrink", "hide"]);
        assert!(!gate.is_pending());
    }

    #[test]
    fn the_hide_waits_the_named_constant() {
        let gate = Arc::new(HideGate::new());
        let timer = FakeTimer::default();
        dismiss(&gate, &timer, &Log::default());
        assert_eq!(timer.delay_of(0), MAPPED_SHRINK_WAIT);
        assert_eq!(MAPPED_SHRINK_WAIT, Duration::from_millis(150));
    }

    #[test]
    fn a_second_dismiss_in_the_wait_arms_no_second_timer_and_shrinks_once() {
        let gate = Arc::new(HideGate::new());
        let timer = FakeTimer::default();
        let log = Log::default();
        dismiss(&gate, &timer, &log);
        dismiss(&gate, &timer, &log);
        assert_eq!(timer.armed(), 1);
        assert_eq!(logged(&log), ["shrink"]);
    }

    #[test]
    fn a_show_in_the_wait_keeps_the_launcher_from_hiding() {
        let gate = Arc::new(HideGate::new());
        let timer = FakeTimer::default();
        let log = Log::default();
        dismiss(&gate, &timer, &log);
        assert!(gate.cancel());
        timer.run(0);
        assert_eq!(logged(&log), ["shrink"]);
    }

    #[test]
    fn the_timer_of_a_cancelled_hide_does_not_hide_a_later_dismiss_early() {
        let gate = Arc::new(HideGate::new());
        let timer = FakeTimer::default();
        let log = Log::default();
        dismiss(&gate, &timer, &log);
        gate.cancel();
        dismiss(&gate, &timer, &log);
        assert_eq!(timer.armed(), 2);
        timer.run(0);
        assert_eq!(logged(&log), ["shrink", "shrink"]);
        timer.run(0);
        assert_eq!(logged(&log), ["shrink", "shrink", "hide"]);
    }

    #[test]
    fn a_pending_hide_counts_the_launcher_as_hidden() {
        assert!(counts_as_visible(true, false));
        assert!(!counts_as_visible(true, true));
        assert!(!counts_as_visible(false, false));
        assert!(!counts_as_visible(false, true));
    }
}
