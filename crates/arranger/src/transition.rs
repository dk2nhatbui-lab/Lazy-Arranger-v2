//! Section requests and when they take effect.
//!
//! The blueprint asks for four transition modes and for fill precedence to be
//! stated rather than discovered. This state machine owns the timing decision
//! and nothing else: it does not know about notes, styles or audio.
//!
//! Policy, stated once:
//!
//! * A request is turned into a **due tick** when it is made.
//! * `Immediate` is due at the tick it was requested.
//! * `NextBeat` and `NextBar` use the meter's next boundary at or after that
//!   tick, so a request made exactly on a boundary applies there.
//! * `EndOfSection` is due at the current section's end, which the caller
//!   supplies because only it knows the section length.
//! * When several requests fall due at the same tick, **the latest request
//!   wins**. A fill is a request like any other; to return from a fill the
//!   caller requests the main section with `EndOfSection` before the fill
//!   ends. This keeps fill behaviour explicit instead of hiding a rule.

use music_core::{Tick, TimeSignature};

/// When a requested section starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionMode {
    /// At the tick the request was made.
    Immediate,
    /// At the next beat boundary at or after the request.
    NextBeat,
    /// At the next bar boundary at or after the request.
    NextBar,
    /// At the end of the section that is playing.
    EndOfSection,
}

/// One queued section change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SectionRequest {
    /// Section to play.
    pub section: String,
    /// How the start was chosen.
    pub mode: TransitionMode,
    /// Tick where the change takes effect.
    pub due: Tick,
    /// Order the request was made in; later requests win ties.
    pub sequence: u64,
}

/// The section that is playing plus the requests waiting for their boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransitionState {
    current: String,
    pending: Vec<SectionRequest>,
    next_sequence: u64,
}

impl TransitionState {
    /// Starts in `current` with no pending requests.
    #[must_use]
    pub fn new(current: impl Into<String>) -> Self {
        Self {
            current: current.into(),
            pending: Vec::new(),
            next_sequence: 1,
        }
    }

    /// The section that is playing.
    #[must_use]
    pub fn current(&self) -> &str {
        &self.current
    }

    /// Requests still waiting for their boundary, in request order.
    #[must_use]
    pub fn pending(&self) -> &[SectionRequest] {
        &self.pending
    }

    /// Queues a section change and returns the tick it is due.
    ///
    /// `current_section_end` is only consulted for
    /// [`TransitionMode::EndOfSection`]; a value at or before `at` makes the
    /// change due immediately.
    pub fn request(
        &mut self,
        section: impl Into<String>,
        mode: TransitionMode,
        at: Tick,
        meter: TimeSignature,
        current_section_end: Tick,
    ) -> Tick {
        let due = match mode {
            TransitionMode::Immediate => at,
            TransitionMode::NextBeat => meter.beat_boundary_at_or_after(at),
            TransitionMode::NextBar => meter.bar_boundary_at_or_after(at),
            TransitionMode::EndOfSection => current_section_end.max(at),
        };
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        self.pending.push(SectionRequest {
            section: section.into(),
            mode,
            due,
            sequence,
        });
        due
    }

    /// Applies every request that is due at `now`.
    ///
    /// Returns the new current section when a change happened. Requests that
    /// came due earlier and are still queued are superseded by a later
    /// request that is also due.
    pub fn poll(&mut self, now: Tick) -> Option<String> {
        let winner = self
            .pending
            .iter()
            .filter(|request| request.due <= now)
            .max_by_key(|request| (request.due, request.sequence))?
            .clone();
        // Everything requested before the winner is superseded, whether it is
        // due yet or not; later requests stay queued for their own boundary.
        self.pending
            .retain(|request| request.sequence > winner.sequence);
        self.current = winner.section.clone();
        Some(winner.section)
    }

    /// Drops every pending request, for example on stop.
    pub fn clear(&mut self) {
        self.pending.clear();
    }

    /// True when a request is waiting for a boundary.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (TransitionState, TimeSignature) {
        (TransitionState::new("Main A"), TimeSignature::default())
    }

    #[test]
    fn next_bar_uses_the_bar_boundary() {
        let (mut state, meter) = setup();
        let due = state.request("Main B", TransitionMode::NextBar, 100, meter, 3840);
        assert_eq!(due, 3840);
        assert_eq!(state.poll(3839), None);
        assert_eq!(state.poll(3840), Some(String::from("Main B")));
        assert_eq!(state.current(), "Main B");
    }

    #[test]
    fn next_beat_uses_the_beat_boundary_at_or_after() {
        let (mut state, meter) = setup();
        assert_eq!(
            state.request("Fill In AA", TransitionMode::NextBeat, 960, meter, 3840),
            960,
            "a request on the beat applies on that beat"
        );
        assert_eq!(
            state.request("Fill In AB", TransitionMode::NextBeat, 961, meter, 3840),
            1920
        );
    }

    #[test]
    fn immediate_applies_on_its_own_tick() {
        let (mut state, meter) = setup();
        assert_eq!(
            state.request("Intro", TransitionMode::Immediate, 500, meter, 3840),
            500
        );
        assert_eq!(state.poll(499), None);
        assert_eq!(state.poll(500), Some(String::from("Intro")));
    }

    #[test]
    fn end_of_section_uses_the_supplied_section_end() {
        let (mut state, meter) = setup();
        assert_eq!(
            state.request("Main B", TransitionMode::EndOfSection, 100, meter, 7680),
            7680
        );
        assert_eq!(state.poll(7679), None);
        assert_eq!(state.poll(7680), Some(String::from("Main B")));
    }

    #[test]
    fn a_late_end_of_section_applies_at_once() {
        let (mut state, meter) = setup();
        // The caller says the section already ended at 100, and the request is
        // made at 200: waiting for 100 would deadlock, so it is due at 200.
        assert_eq!(
            state.request("Main B", TransitionMode::EndOfSection, 200, meter, 100),
            200
        );
    }

    #[test]
    fn the_latest_request_wins_at_one_boundary() {
        let (mut state, meter) = setup();
        state.request("Main B", TransitionMode::NextBar, 0, meter, 3840);
        state.request("Fill In AA", TransitionMode::NextBar, 10, meter, 3840);
        assert_eq!(state.poll(3840), Some(String::from("Fill In AA")));
        assert!(!state.has_pending());
    }

    #[test]
    fn an_earlier_boundary_is_not_waiting_for_a_later_one() {
        let (mut state, meter) = setup();
        state.request("Main B", TransitionMode::NextBar, 0, meter, 3840);
        let later_due = state.request("Main C", TransitionMode::NextBeat, 3850, meter, 3840);
        assert_eq!(later_due, 4800, "the next beat boundary after 3850");
        assert_eq!(state.poll(3840), Some(String::from("Main B")));
        // Main C was requested after the winner, so it stays queued.
        assert!(state.has_pending());
        assert_eq!(state.poll(4799), None);
        assert_eq!(state.poll(4800), Some(String::from("Main C")));
    }

    #[test]
    fn clear_drops_pending_requests() {
        let (mut state, meter) = setup();
        state.request("Main B", TransitionMode::NextBar, 0, meter, 3840);
        assert!(state.has_pending());
        state.clear();
        assert!(!state.has_pending());
        assert_eq!(state.poll(10_000), None);
    }

    #[test]
    fn a_request_beyond_now_stays_queued() {
        let (mut state, meter) = setup();
        // A request made at tick 1 waits for the next bar, which is 3840.
        assert_eq!(
            state.request("Main B", TransitionMode::NextBar, 1, meter, 3840),
            3840
        );
        assert_eq!(state.poll(100), None);
        assert_eq!(state.pending().len(), 1);
        assert_eq!(state.current(), "Main A");
    }
}
