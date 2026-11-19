//! The machine's clock: a count of cycles, and the events the devices schedule against it. Time
//! only moves when the run that carries the program says so, by the cycles its code took, so a
//! run is the same run every time.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// The events of a machine in the order they come due; two due at one cycle come in the order
/// they were scheduled.
#[derive(Debug)]
pub struct Scheduler<E> {
    now: u64,
    order: u64,
    events: BinaryHeap<Reverse<(u64, u64, E)>>,
}

impl<E: Ord> Default for Scheduler<E> {
    fn default() -> Self {
        Self { now: 0, order: 0, events: BinaryHeap::new() }
    }
}

impl<E: Ord> Scheduler<E> {
    /// The cycles since the machine started.
    pub fn now(&self) -> u64 {
        self.now
    }

    /// The cycles until the next event comes due; `None` with none scheduled.
    pub fn until_next(&self) -> Option<u64> {
        self.events.peek().map(|Reverse((due, ..))| due.saturating_sub(self.now))
    }

    /// Schedules `event` for `after` cycles from now.
    pub fn after(&mut self, after: u64, event: E) {
        self.order += 1;
        self.events.push(Reverse((self.now + after, self.order, event)));
    }

    /// Drops every scheduled event `drop` names.
    pub fn cancel(&mut self, drop: impl Fn(&E) -> bool) {
        let kept: Vec<_> = std::mem::take(&mut self.events).into_iter().filter(|Reverse((.., e))| !drop(e)).collect();
        self.events = kept.into();
    }

    /// Moves the clock `cycles` on, to the next event when one comes due within them: the event,
    /// with the clock at its cycle, or `None` with the clock at the end. A caller loops until
    /// `None`, with what is left of its cycles.
    pub fn step(&mut self, cycles: &mut u64) -> Option<E> {
        let end = self.now + *cycles;
        match self.events.peek() {
            Some(Reverse((due, ..))) if *due <= end => {
                let Reverse((due, _, event)) = self.events.pop()?;
                // An event scheduled in the past (by a handler that ran late) is due now.
                let due = due.max(self.now);
                *cycles = end - due;
                self.now = due;
                Some(event)
            }
            _ => {
                self.now = end;
                *cycles = 0;
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_come_due_in_order_and_the_clock_stops_at_each() {
        let mut clock = Scheduler::default();
        clock.after(30, "late");
        clock.after(10, "first");
        clock.after(10, "second");
        let mut cycles = 25;
        assert_eq!(clock.step(&mut cycles), Some("first"));
        assert_eq!((clock.now(), cycles), (10, 15));
        assert_eq!(clock.step(&mut cycles), Some("second"));
        assert_eq!(clock.step(&mut cycles), None);
        assert_eq!((clock.now(), cycles), (25, 0));
        let mut cycles = 5;
        assert_eq!(clock.step(&mut cycles), Some("late"));
        assert_eq!(clock.now(), 30);
    }

    #[test]
    fn a_cancelled_event_never_comes() {
        let mut clock = Scheduler::default();
        clock.after(5, 1);
        clock.after(6, 2);
        clock.cancel(|e| *e == 1);
        let mut cycles = 10;
        assert_eq!(clock.step(&mut cycles), Some(2));
        assert_eq!(clock.step(&mut cycles), None);
    }
}
