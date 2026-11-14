//! Conveyor kinematics.
//!
//! A conveyor is modelled as a one-dimensional belt of length `length` on which
//! items move at `speed` towards the exit. The engine keeps the flow items; this
//! module only keeps their positions (distance from the entry point) and
//! answers the three questions the event calendar needs:
//!
//! * how long until the next item reaches a constraint (`time_to_next_event`),
//! * move every item forward by a time step (`advance`),
//! * is there room at the entry / is an item ready to leave (`entry_free`,
//!   `front_ready`).
//!
//! Positions are stored front first (`positions[0]` is the item nearest to the
//! exit), so the exit is at `length` and the entry at `0`.
//!
//! Two accumulation policies are supported:
//!
//! * **accumulating**: an item stops when it reaches the exit or when it would
//!   come closer than `item_spacing` to the item in front. Items behind it keep
//!   moving until they queue up, so the belt can hold a compressed queue.
//! * **non-accumulating**: the belt is a rigid train. When the leading item
//!   cannot leave, the whole belt stops; items never close the gap.

/// Tolerance used for "reached the exit" / "touching the item in front".
const EPS: f64 = 1e-9;

#[derive(Debug, Clone, PartialEq)]
pub struct Belt {
    pub length: f64,
    pub speed: f64,
    pub spacing: f64,
    pub accumulating: bool,
    /// Item positions, front first.
    pub positions: Vec<f64>,
    /// Remaining station dwell time per item (front first, aligned with
    /// `positions`); `0` means the item is moving. B3 stations stop an item at
    /// a fixed position for a while without blocking the whole belt.
    pub holds: Vec<f64>,
}

impl Belt {
    pub fn new(length: f64, speed: f64, spacing: f64, accumulating: bool) -> Self {
        Self {
            length: length.max(EPS),
            speed: speed.max(EPS),
            spacing: spacing.max(0.0),
            accumulating,
            positions: Vec::new(),
            holds: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.positions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    /// The leading item has reached the exit and may be transferred.
    pub fn front_ready(&self) -> bool {
        self.positions
            .first()
            .is_some_and(|pos| *pos >= self.length - EPS)
    }

    /// A new item fits on the belt surface. `max_content` is the hard limit;
    /// the spacing rule keeps the entry zone free.
    pub fn entry_free(&self, max_content: u32) -> bool {
        if self.positions.len() >= max_content as usize {
            return false;
        }
        match self.positions.last() {
            None => true,
            Some(&back) => back >= self.spacing - EPS,
        }
    }

    /// Put a new item on the entry point.
    pub fn admit(&mut self) {
        self.positions.push(0.0);
        self.holds.push(0.0);
    }

    /// Remove the leading item (it left the belt).
    pub fn remove_front(&mut self) {
        if !self.positions.is_empty() {
            self.positions.remove(0);
        }
        if !self.holds.is_empty() {
            self.holds.remove(0);
        }
    }

    /// Position of one item (front first).
    pub fn position(&self, index: usize) -> Option<f64> {
        self.positions.get(index).copied()
    }

    /// Move one item to `position` (used to clamp it exactly onto a stop).
    pub fn set_position(&mut self, index: usize, position: f64) {
        if let Some(slot) = self.positions.get_mut(index) {
            *slot = position;
        }
    }

    /// Remaining station dwell time of one item.
    pub fn hold_remaining(&self, index: usize) -> f64 {
        self.holds.get(index).copied().unwrap_or(0.0)
    }

    /// Start a station dwell for one item.
    pub fn set_hold(&mut self, index: usize, seconds: f64) {
        if let Some(slot) = self.holds.get_mut(index) {
            *slot = seconds.max(0.0);
        }
    }

    /// The furthest position an item may reach, given the item in front.
    fn limit(&self, index: usize) -> f64 {
        if index == 0 {
            self.length
        } else {
            (self.positions[index - 1] - self.spacing).max(self.positions[index])
        }
    }

    /// The belt is jammed: the leading item is at the exit and the belt is
    /// non-accumulating, so nothing moves.
    pub fn frozen(&self) -> bool {
        !self.accumulating && self.front_ready()
    }

    /// Time until the next item reaches a constraint. `None` means the belt is
    /// stationary until an external event (an exit or an entry) happens.
    pub fn time_to_next_event(&self) -> Option<f64> {
        self.time_to_next_event_with_stops(&[])
    }

    /// Like [`Belt::time_to_next_event`], but also stops at the given belt
    /// positions (photo eyes and stations, B3) and at station dwell expiries.
    ///
    /// `stops` must be sorted ascending; positions the item already passed are
    /// ignored.
    pub fn time_to_next_event_with_stops(&self, stops: &[f64]) -> Option<f64> {
        if self.frozen() {
            return None;
        }
        let mut next: Option<f64> = None;
        let mut consider = |time: f64| {
            if time > EPS {
                next = Some(match next {
                    Some(current) => current.min(time),
                    None => time,
                });
            }
        };
        for index in 0..self.positions.len() {
            let hold = self.hold_remaining(index);
            if hold > EPS {
                consider(hold);
                continue;
            }
            let mut target = self.limit(index);
            if let Some(stop) = stops
                .iter()
                .copied()
                .filter(|stop| *stop > self.positions[index] + EPS)
                .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            {
                target = target.min(stop);
            }
            let gap = target - self.positions[index];
            if gap > EPS {
                consider(gap / self.speed);
            }
        }
        next.map(|time| time.max(0.0))
    }

    /// Move all items forward by `dt`, respecting the spacing, stop and exit
    /// limits. Items dwelling at a station consume `dt` before they move.
    pub fn advance(&mut self, dt: f64) {
        if dt <= 0.0 || self.frozen() {
            return;
        }
        let step = self.speed * dt;
        for index in 0..self.positions.len() {
            let hold = self.hold_remaining(index);
            if hold > 0.0 {
                // The dwell uses the whole step; the item starts moving at the
                // next event (the dwell expiry).
                if let Some(slot) = self.holds.get_mut(index) {
                    *slot = (hold - dt).max(0.0);
                }
                continue;
            }
            let limit = self.limit(index);
            let target = (self.positions[index] + step).min(limit);
            self.positions[index] = target.max(self.positions[index]);
        }
    }

    /// Sum of the distances covered, useful for diagnostics.
    pub fn occupied_length(&self) -> f64 {
        self.positions.first().copied().unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn travel_time_is_length_over_speed() {
        let mut belt = Belt::new(10.0, 2.0, 1.0, true);
        belt.admit();
        // 10 distance units at 2 units per time -> 5 time units.
        let time = belt.time_to_next_event().expect("the item moves");
        assert!((time - 5.0).abs() < 1e-9, "expected 5, got {time}");
        belt.advance(5.0);
        assert!(belt.front_ready());
    }

    #[test]
    fn spacing_stops_items_behind_a_blocked_leader() {
        let mut belt = Belt::new(10.0, 1.0, 2.0, true);
        // Front item at the exit, second item 5 units behind it.
        belt.positions = vec![10.0, 5.0];
        // The second item may close the gap until it is `spacing` from the
        // leader: 10 - 2 - 5 = 3 units, at speed 1 -> 3 time units.
        let time = belt.time_to_next_event().expect("the second item moves");
        assert!((time - 3.0).abs() < 1e-9, "expected 3, got {time}");
        belt.advance(3.0);
        assert!((belt.positions[1] - 8.0).abs() < 1e-9);
        // Now the belt is compacted: no further movement.
        assert_eq!(belt.time_to_next_event(), None);
    }

    #[test]
    fn non_accumulating_belt_freezes_as_a_whole() {
        let mut belt = Belt::new(10.0, 1.0, 1.0, false);
        belt.positions = vec![10.0, 4.0];
        assert!(belt.frozen());
        assert_eq!(belt.time_to_next_event(), None);
        belt.advance(0.5);
        assert!(
            (belt.positions[1] - 4.0).abs() < 1e-9,
            "the train must not move"
        );
        // Once the leader leaves, the train moves again.
        belt.remove_front();
        let next = belt.time_to_next_event().expect("the train moves again");
        assert!((next - 6.0).abs() < 1e-9);
    }

    #[test]
    fn entry_needs_spacing_and_capacity() {
        let mut belt = Belt::new(10.0, 1.0, 2.0, true);
        assert!(belt.entry_free(4));
        belt.admit();
        assert!(!belt.entry_free(4), "the entry point is occupied");
        belt.advance(1.0);
        assert!((belt.positions[0] - 1.0).abs() < 1e-9);
        assert!(
            !belt.entry_free(4),
            "one unit is closer than the 2 unit spacing"
        );
        belt.advance(1.0);
        assert!(belt.entry_free(4));
        belt.positions = vec![9.0, 8.0, 7.0, 6.0];
        assert!(!belt.entry_free(4), "capacity is exhausted");
    }

    #[test]
    fn accumulating_belt_compresses_towards_the_exit() {
        let mut belt = Belt::new(10.0, 1.0, 1.0, true);
        belt.positions = vec![10.0, 9.0, 8.0];
        // All items are already packed, so nothing moves even though the
        // leader is waiting at the exit.
        assert_eq!(belt.time_to_next_event(), None);
        assert!(!belt.frozen(), "accumulating belts are not rigid");
        belt.remove_front();
        let next = belt.time_to_next_event().expect("the next item moves up");
        assert!((next - 1.0).abs() < 1e-9);
    }
}

#[cfg(test)]
mod stop_tests {
    use super::*;

    #[test]
    fn stops_are_reached_at_position_over_speed() {
        let mut belt = Belt::new(10.0, 2.0, 1.0, true);
        belt.admit();
        // 4 distance units at 2 units per time unit.
        let dt = belt.time_to_next_event_with_stops(&[4.0]).expect("eye");
        assert!((dt - 2.0).abs() < 1e-9);
        belt.advance(dt);
        assert!((belt.position(0).expect("item") - 4.0).abs() < 1e-9);
        // The stop is behind the item now, so the exit is next.
        let dt = belt.time_to_next_event_with_stops(&[4.0]).expect("exit");
        assert!((dt - 3.0).abs() < 1e-9);
    }

    #[test]
    fn a_station_dwell_pauses_only_that_item() {
        let mut belt = Belt::new(10.0, 1.0, 1.0, true);
        belt.admit();
        belt.positions = vec![5.0];
        belt.holds = vec![3.0];
        // The dwell expires before the exit is reached.
        let dt = belt.time_to_next_event_with_stops(&[5.0]).expect("dwell");
        assert!((dt - 3.0).abs() < 1e-9);
        belt.advance(dt);
        assert!((belt.hold_remaining(0) - 0.0).abs() < 1e-9);
        assert!((belt.position(0).expect("item") - 5.0).abs() < 1e-9);
        // Moving again: 5 more units to the exit.
        let dt = belt.time_to_next_event_with_stops(&[5.0]).expect("exit");
        assert!((dt - 5.0).abs() < 1e-9);
    }

    #[test]
    fn a_dwelling_item_holds_back_the_item_behind() {
        let mut belt = Belt::new(10.0, 1.0, 1.0, true);
        belt.positions = vec![5.0, 3.0];
        belt.holds = vec![100.0, 0.0];
        // The second item may close the gap to 4.0, not closer than the
        // spacing to the dwelling leader.
        let dt = belt.time_to_next_event_with_stops(&[]).expect("gap");
        assert!((dt - 1.0).abs() < 1e-9);
        belt.advance(dt);
        assert!((belt.position(1).expect("item") - 4.0).abs() < 1e-9);
        // The only remaining boundary is the leader's dwell expiry (99 left).
        let next = belt
            .time_to_next_event_with_stops(&[])
            .expect("dwell expiry");
        assert!((next - 99.0).abs() < 1e-9);
    }
}
