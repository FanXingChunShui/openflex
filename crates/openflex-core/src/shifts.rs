//! Work shifts (planned availability windows).
//!
//! An object with a non-empty shift plan is only available inside the windows
//! it declares. Outside them the engine reports the object as `off_shift`,
//! pauses its processing and refuses new flow items until the next window
//! starts.
//!
//! A [`Shift`] is `[start, start + duration)` and, when `repeat_every > 0`,
//! repeats with that period: window `k` covers
//! `[start + k * period, start + k * period + duration)`.

use openflex_model::Shift;

/// Numerical tolerance for window boundaries.
const EPS: f64 = 1e-9;

/// Is `time` inside one of the windows?
pub fn is_active(shifts: &[Shift], time: f64) -> bool {
    if shifts.is_empty() {
        return true;
    }
    shifts.iter().any(|shift| in_window(shift, time))
}

fn in_window(shift: &Shift, time: f64) -> bool {
    if shift.duration <= 0.0 || time + EPS < shift.start {
        return false;
    }
    let offset = time - shift.start;
    if shift.repeat_every <= 0.0 {
        return offset < shift.duration;
    }
    let cycles = (offset / shift.repeat_every).floor();
    let within = offset - cycles * shift.repeat_every;
    within < shift.duration
}

/// The first window boundary (start or end) strictly after `after`.
///
/// The engine schedules one event per boundary, so an object wakes up exactly
/// when its availability changes.
pub fn next_transition(shifts: &[Shift], after: f64) -> Option<f64> {
    let mut next: Option<f64> = None;
    for shift in shifts {
        if shift.duration <= 0.0 {
            continue;
        }
        let candidate = next_for(shift, after);
        if let Some(time) = candidate {
            next = Some(match next {
                Some(current) => current.min(time),
                None => time,
            });
        }
    }
    next
}

fn next_for(shift: &Shift, after: f64) -> Option<f64> {
    let start = shift.start;
    let end = start + shift.duration;
    let mut best: Option<f64> = None;
    let mut consider = |time: f64| {
        if time > after + EPS {
            best = Some(match best {
                Some(current) => current.min(time),
                None => time,
            });
        }
    };

    if shift.repeat_every <= 0.0 {
        consider(start);
        consider(end);
        return best;
    }
    let period = shift.repeat_every;
    if after < start {
        consider(start);
        return best;
    }
    // Only the occurrence containing `after` and the following one can produce
    // the next boundary.
    let first = ((after - start) / period).floor().max(0.0);
    for cycle in [first, first + 1.0] {
        consider(start + cycle * period);
        consider(start + cycle * period + shift.duration);
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day_shift() -> Vec<Shift> {
        vec![Shift {
            start: 0.0,
            duration: 480.0,
            repeat_every: 1440.0,
        }]
    }

    #[test]
    fn empty_plan_is_always_active() {
        assert!(is_active(&[], 0.0));
        assert!(is_active(&[], 1e9));
        assert_eq!(next_transition(&[], 0.0), None);
    }

    #[test]
    fn a_shift_covers_its_window_only() {
        let shifts = day_shift();
        assert!(is_active(&shifts, 0.0));
        assert!(is_active(&shifts, 479.999));
        assert!(!is_active(&shifts, 480.0));
        assert!(!is_active(&shifts, 1439.0));
        assert!(is_active(&shifts, 1440.0));
        assert!(is_active(&shifts, 1440.0 + 100.0));
    }

    #[test]
    fn transitions_alternate_start_and_end() {
        let shifts = day_shift();
        assert_eq!(next_transition(&shifts, -1.0), Some(0.0));
        assert_eq!(next_transition(&shifts, 0.0), Some(480.0));
        assert_eq!(next_transition(&shifts, 480.0), Some(1440.0));
        assert_eq!(next_transition(&shifts, 1500.0), Some(1920.0));
    }

    #[test]
    fn a_single_window_ends_permanently() {
        let shifts = vec![Shift {
            start: 10.0,
            duration: 5.0,
            repeat_every: 0.0,
        }];
        assert!(!is_active(&shifts, 9.0));
        assert!(is_active(&shifts, 10.0));
        assert!(!is_active(&shifts, 15.0));
        assert_eq!(next_transition(&shifts, 15.0), None);
    }

    #[test]
    fn zero_duration_shifts_are_ignored() {
        let shifts = vec![Shift {
            start: 0.0,
            duration: 0.0,
            repeat_every: 10.0,
        }];
        assert!(!is_active(&shifts, 0.0));
        assert_eq!(next_transition(&shifts, 0.0), None);
    }
}
