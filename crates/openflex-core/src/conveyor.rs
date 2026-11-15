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

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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
    /// task-56: end positions of the multi-speed bands (ascending, the last
    /// band ends at `length`). Empty when the belt runs at a single `speed`.
    #[serde(default)]
    pub segment_bounds: Vec<f64>,
    /// Speed of each band; `segment_bounds.len() + 1 == segment_speeds.len()`
    /// when the belt is segmented.
    #[serde(default)]
    pub segment_speeds: Vec<f64>,
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
            segment_bounds: Vec::new(),
            segment_speeds: Vec::new(),
        }
    }

    /// task-56: install a multi-speed profile. `bounds` are the end positions
    /// of the bands (ascending, below `length`); `speeds` has one entry per
    /// band (`bounds.len() + 1`).
    pub fn set_segments(&mut self, bounds: Vec<f64>, speeds: Vec<f64>) {
        if speeds.len() == bounds.len() + 1 && !speeds.is_empty() {
            self.segment_bounds = bounds;
            self.segment_speeds = speeds.into_iter().map(|speed| speed.max(EPS)).collect();
        }
    }

    fn segmented(&self) -> bool {
        self.segment_speeds.len() == self.segment_bounds.len() + 1
            && !self.segment_speeds.is_empty()
    }

    /// Belt speed at `position` (task-56).
    pub fn speed_at(&self, position: f64) -> f64 {
        if !self.segmented() {
            return self.speed;
        }
        let index = self
            .segment_bounds
            .iter()
            .filter(|bound| position >= **bound - EPS)
            .count();
        self.segment_speeds[index.min(self.segment_speeds.len() - 1)]
    }

    /// Position after moving `dt` from `from`, integrating each band's speed
    /// (task-56).
    pub fn advance_position(&self, from: f64, dt: f64) -> f64 {
        if !self.segmented() {
            return from + self.speed * dt;
        }
        let mut position = from;
        let mut remaining = dt;
        let mut guard = 0;
        while remaining > EPS && guard < 10_000 {
            guard += 1;
            let speed = self.speed_at(position);
            let next_bound = self
                .segment_bounds
                .iter()
                .copied()
                .find(|bound| *bound > position + EPS);
            let distance = speed * remaining;
            match next_bound {
                Some(bound) if position + distance > bound => {
                    remaining -= (bound - position) / speed;
                    position = bound;
                }
                _ => {
                    position += distance;
                    remaining = 0.0;
                }
            }
        }
        position
    }

    /// Time to travel from `from` to `to`, integrating each band's speed
    /// (task-56).
    pub fn time_to_reach(&self, from: f64, to: f64) -> f64 {
        if to <= from + EPS {
            return 0.0;
        }
        if !self.segmented() {
            return (to - from) / self.speed;
        }
        let mut position = from;
        let mut total = 0.0;
        let mut guard = 0;
        while position < to - EPS && guard < 10_000 {
            guard += 1;
            let speed = self.speed_at(position);
            let next_bound = self
                .segment_bounds
                .iter()
                .copied()
                .find(|bound| *bound > position + EPS);
            let stop_at = match next_bound {
                Some(bound) if bound < to => bound,
                _ => to,
            };
            total += (stop_at - position) / speed;
            position = stop_at;
        }
        total
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

    /// task-71: enforce the minimum spacing from front to back by pushing
    /// items backwards (`pos[i] <= pos[i-1] - spacing`). Items that were
    /// placed too close are compressed onto the spacing grid instead of
    /// overlapping.
    fn compress(&mut self) {
        for index in 1..self.positions.len() {
            let limit = self.positions[index - 1] - self.spacing;
            if self.positions[index] > limit {
                self.positions[index] = limit.max(0.0);
            }
        }
        // The last item may have been pushed behind the entry.
        if let Some(last) = self.positions.last_mut()
            && *last < 0.0
        {
            *last = 0.0;
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
                // task-56: integrate the speed profile.
                consider(self.time_to_reach(self.positions[index], target));
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
        // task-71: strict pushing - items can never overlap, even when a model
        // placed them closer than `spacing`.
        self.compress();
        // task-59: advance in the smallest constrained step instead of
        // consuming the whole `dt`. An item that is dwelling at a station or
        // waiting for the item in front keeps its remaining time and moves on
        // once the constraint clears, so `advance(5)` with a 2 s dwell still
        // moves the item for the remaining 3 s.
        let mut remaining = dt;
        let mut guard = 0;
        while remaining > EPS && guard < 10_000 {
            guard += 1;
            let mut step = remaining;
            let mut has_moving = false;
            for index in 0..self.positions.len() {
                let hold = self.hold_remaining(index);
                if hold > EPS {
                    // The dwell ends after `hold`; the item may move afterwards.
                    has_moving = true;
                    step = step.min(hold);
                    continue;
                }
                let gap = self.limit(index) - self.positions[index];
                if gap <= EPS {
                    // Jammed against its limit for now; other items may still
                    // have room, so it does not stop the whole step.
                    continue;
                }
                has_moving = true;
                step = step.min(self.time_to_reach(self.positions[index], self.limit(index)));
            }
            if step <= EPS {
                break;
            }
            let step = step.min(remaining);
            for index in 0..self.positions.len() {
                let hold = self.hold_remaining(index);
                if hold > EPS {
                    if let Some(slot) = self.holds.get_mut(index) {
                        *slot = (hold - step).max(0.0);
                    }
                    continue;
                }
                let limit = self.limit(index);
                let target = self
                    .advance_position(self.positions[index], step)
                    .min(limit);
                self.positions[index] = target.max(self.positions[index]);
            }
            remaining -= step;
            if !has_moving {
                // Every item is jammed: the rest of `dt` cannot move anything.
                break;
            }
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

    /// task-56: a segmented belt integrates each band's speed, so the item
    /// reaches the band boundary exactly when that band's travel time says.
    #[test]
    fn segmented_belt_integrates_speed_per_band() {
        let mut belt = Belt::new(20.0, 1.0, 1.0, true);
        belt.set_segments(vec![10.0], vec![1.0, 2.0]);
        // First band: 10 units at speed 1.
        assert!((belt.time_to_reach(0.0, 10.0) - 10.0).abs() < 1e-9);
        // Second band: 5 more units at speed 2.
        assert!((belt.time_to_reach(0.0, 15.0) - 12.5).abs() < 1e-9);
        // The whole belt: 10 + 5.
        assert!((belt.time_to_reach(0.0, 20.0) - 15.0).abs() < 1e-9);
        // Moving 4 seconds from position 8: 2 s to the boundary, then 2 s at
        // speed 2.
        assert!((belt.advance_position(8.0, 4.0) - 14.0).abs() < 1e-9);
        // A single-speed belt would have moved 8 + 4 = 12.
        assert!((belt.advance_position(8.0, 4.0) - 12.0).abs() > 1e-9);
    }

    /// A fast-first belt reaches the boundary earlier than a slow-first one.
    #[test]
    fn segmented_belt_order_matters_for_intermediate_positions() {
        let mut fast_first = Belt::new(20.0, 1.0, 1.0, true);
        fast_first.set_segments(vec![10.0], vec![2.0, 1.0]);
        let mut slow_first = Belt::new(20.0, 1.0, 1.0, true);
        slow_first.set_segments(vec![10.0], vec![1.0, 2.0]);
        // Both take 15 time units for the whole belt ...
        assert!((fast_first.time_to_reach(0.0, 20.0) - 15.0).abs() < 1e-9);
        assert!((slow_first.time_to_reach(0.0, 20.0) - 15.0).abs() < 1e-9);
        // ... but the first half is reached after 5 s, not after 10 s.
        assert!((fast_first.time_to_reach(0.0, 10.0) - 5.0).abs() < 1e-9);
        assert!((slow_first.time_to_reach(0.0, 10.0) - 10.0).abs() < 1e-9);
    }

    /// task-59: after a station dwell the item keeps the remaining time of the
    /// step instead of losing it.
    #[test]
    fn advance_after_a_dwell_uses_the_remaining_time() {
        let mut belt = Belt::new(10.0, 1.0, 1.0, true);
        belt.admit();
        belt.set_hold(0, 2.0);
        belt.advance(5.0);
        // 2 s of dwelling + 3 s of travel at speed 1.
        assert!((belt.position(0).unwrap() - 3.0).abs() < 1e-9);
    }

    /// task-59: a blocked item starts moving as soon as the leader makes room.
    #[test]
    fn advance_resumes_a_blocked_item_when_the_leader_moves() {
        let mut belt = Belt::new(10.0, 1.0, 1.0, true);
        belt.admit();
        belt.admit();
        belt.set_position(0, 5.0);
        belt.set_position(1, 3.0);
        belt.advance(10.0);
        // The leader reaches the exit at t = 5, the follower keeps one spacing
        // behind and reaches 9.
        assert!((belt.position(0).unwrap() - 10.0).abs() < 1e-9);
        assert!((belt.position(1).unwrap() - 9.0).abs() < 1e-9);
    }
}
#[cfg(test)]
mod push_tests {
    use super::*;

    /// task-71: three items closer than the spacing are compressed onto the
    /// spacing grid before they move, so they never overlap.
    #[test]
    fn advance_compresses_overlapping_items() {
        let mut belt = Belt::new(20.0, 1.0, 1.0, true);
        belt.admit();
        belt.admit();
        belt.admit();
        belt.set_position(0, 2.0);
        belt.set_position(1, 1.5);
        belt.set_position(2, 1.0);
        belt.advance(1.0);
        // The train is re-spaced, then moves as a whole.
        assert!((belt.position(0).unwrap() - 3.0).abs() < 1e-9);
        assert!((belt.position(1).unwrap() - 2.0).abs() < 1e-9);
        assert!((belt.position(2).unwrap() - 1.0).abs() < 1e-9);
        for index in 1..3 {
            let gap = belt.position(index - 1).unwrap() - belt.position(index).unwrap();
            assert!(gap >= 1.0 - 1e-9, "items must not overlap, gap {gap}");
        }
    }

    /// Compression does not change the travel time of a well-spaced belt.
    #[test]
    fn compress_does_not_change_travel_time() {
        let mut belt = Belt::new(10.0, 2.0, 1.0, true);
        belt.admit();
        belt.advance(5.0);
        assert!((belt.position(0).unwrap() - 10.0).abs() < 1e-9);
    }
}
