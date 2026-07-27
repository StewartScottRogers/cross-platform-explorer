//! Scrubber transport model for activity replay (CPE-1064, epic CPE-728).
//!
//! Pure `ts`-only playback transport over [`AuditEvent`](crate::audit_journal::AuditEvent):
//! step the playhead to the next/previous **distinct** event timestamp, slice a half-open
//! playback window `[t0, t1)`, and advance the cursor by an elapsed wall-clock delta scaled by a
//! playback speed. Independent of the projection core — the replay UI's scrubbing math lives
//! here so it's exhaustively unit-tested without any GUI or filesystem dependency.
//!
//! Std-only, no new dependencies. Events are assumed sorted by `ts`; callers that can't guarantee
//! that should sort defensively before calling — the functions here don't mutate their input.

use crate::audit_journal::AuditEvent;

/// Next **distinct** event timestamp strictly after `cursor`, or `None` if `cursor` is at or past
/// the last event. Duplicate timestamps equal to the first one found after `cursor` are skipped
/// until a strictly greater value appears.
pub fn step_next(events: &[AuditEvent], cursor: u64) -> Option<u64> {
    events.iter().map(|e| e.ts).filter(|&ts| ts > cursor).min()
}

/// Previous **distinct** event timestamp strictly before `cursor`, or `None` if `cursor` is at or
/// before the first event.
pub fn step_prev(events: &[AuditEvent], cursor: u64) -> Option<u64> {
    events.iter().map(|e| e.ts).filter(|&ts| ts < cursor).max()
}

/// Half-open `[t0, t1)` slice of `events` for one playback tick: an event at exactly `t0` is
/// included, one at exactly `t1` is excluded. Returns an empty `Vec` if `t1 <= t0`.
pub fn events_in_window(events: &[AuditEvent], t0: u64, t1: u64) -> Vec<&AuditEvent> {
    if t1 <= t0 {
        return Vec::new();
    }
    events.iter().filter(|e| e.ts >= t0 && e.ts < t1).collect()
}

/// Advance `cursor` by `delta_ms * (speed_num / speed_den)`, clamped to `[start, end]`.
///
/// Uses saturating arithmetic throughout so an arbitrarily large `delta_ms`/`speed_num` from the
/// UI **saturates to `end`, never panics or wraps**. `speed_den == 0` or `speed_num == 0` is
/// treated as a no-op: the cursor is returned unchanged except for being clamped into range.
pub fn advance(
    cursor: u64,
    delta_ms: u64,
    speed_num: u64,
    speed_den: u64,
    start: u64,
    end: u64,
) -> u64 {
    let (lo, hi) = if start <= end { (start, end) } else { (end, start) };
    let clamp = |v: u64| -> u64 {
        if v < lo {
            lo
        } else if v > hi {
            hi
        } else {
            v
        }
    };

    if speed_den == 0 || speed_num == 0 {
        return clamp(cursor);
    }

    let scaled = delta_ms.saturating_mul(speed_num) / speed_den;
    clamp(cursor.saturating_add(scaled))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(ts: u64) -> AuditEvent {
        AuditEvent {
            ts,
            session: "s".into(),
            kind: "modified".into(),
            path: "/x".into(),
            actor: None,
            detail: None,
        }
    }

    // ---- step_next / step_prev ----

    #[test]
    fn step_next_lands_on_next_distinct_ts() {
        let events = vec![ev(10), ev(20), ev(20), ev(30)];
        assert_eq!(step_next(&events, 10), Some(20));
        assert_eq!(step_next(&events, 20), Some(30));
    }

    #[test]
    fn step_next_none_past_end() {
        let events = vec![ev(10), ev(20), ev(30)];
        assert_eq!(step_next(&events, 30), None);
        assert_eq!(step_next(&events, 100), None);
    }

    #[test]
    fn step_next_skips_duplicate_ts_to_next_distinct_value() {
        let events = vec![ev(10), ev(10), ev(10), ev(20)];
        assert_eq!(step_next(&events, 10), Some(20));
    }

    #[test]
    fn step_next_from_before_first_event() {
        let events = vec![ev(10), ev(20)];
        assert_eq!(step_next(&events, 0), Some(10));
    }

    #[test]
    fn step_prev_lands_on_prev_distinct_ts() {
        let events = vec![ev(10), ev(20), ev(20), ev(30)];
        assert_eq!(step_prev(&events, 30), Some(20));
        assert_eq!(step_prev(&events, 20), Some(10));
    }

    #[test]
    fn step_prev_none_past_start() {
        let events = vec![ev(10), ev(20), ev(30)];
        assert_eq!(step_prev(&events, 10), None);
        assert_eq!(step_prev(&events, 0), None);
    }

    #[test]
    fn step_prev_skips_duplicate_ts_to_prev_distinct_value() {
        let events = vec![ev(10), ev(20), ev(20), ev(20)];
        assert_eq!(step_prev(&events, 20), Some(10));
    }

    #[test]
    fn step_next_prev_empty_events() {
        let events: Vec<AuditEvent> = vec![];
        assert_eq!(step_next(&events, 0), None);
        assert_eq!(step_prev(&events, 0), None);
    }

    // ---- events_in_window ----

    #[test]
    fn events_in_window_is_half_open() {
        let events = vec![ev(5), ev(10), ev(15), ev(20)];
        let win = events_in_window(&events, 10, 20);
        let tss: Vec<u64> = win.iter().map(|e| e.ts).collect();
        assert_eq!(tss, vec![10, 15]); // t0 included, t1 excluded
    }

    #[test]
    fn events_in_window_empty_when_t1_le_t0() {
        let events = vec![ev(5), ev(10)];
        assert!(events_in_window(&events, 10, 10).is_empty());
        assert!(events_in_window(&events, 20, 10).is_empty());
    }

    #[test]
    fn events_in_window_excludes_outside_range() {
        let events = vec![ev(1), ev(50)];
        assert!(events_in_window(&events, 10, 20).is_empty());
    }

    // ---- advance ----

    #[test]
    fn advance_within_range() {
        assert_eq!(advance(100, 50, 1, 1, 0, 1000), 150);
    }

    #[test]
    fn advance_scales_by_speed() {
        // 2x speed: 100ms elapsed * 2/1 = 200ms of playhead movement.
        assert_eq!(advance(0, 100, 2, 1, 0, 1000), 200);
        // Half speed: 100ms elapsed * 1/2 = 50ms of playhead movement.
        assert_eq!(advance(0, 100, 1, 2, 0, 1000), 50);
    }

    #[test]
    fn advance_past_end_clamps_to_end() {
        assert_eq!(advance(900, 500, 1, 1, 0, 1000), 1000);
    }

    #[test]
    fn advance_below_start_clamps_to_start() {
        // cursor already below start, zero movement -> clamped up to start.
        assert_eq!(advance(5, 0, 1, 1, 10, 1000), 10);
    }

    #[test]
    fn advance_speed_zero_is_noop() {
        assert_eq!(advance(100, 500, 0, 1, 0, 1000), 100);
    }

    #[test]
    fn advance_den_zero_is_noop() {
        assert_eq!(advance(100, 500, 1, 0, 0, 1000), 100);
    }

    #[test]
    fn advance_noop_still_clamps_out_of_range_cursor() {
        // Even a no-op (speed 0) clamps an out-of-range cursor into [start, end].
        assert_eq!(advance(2000, 500, 0, 1, 0, 1000), 1000);
        assert_eq!(advance(2000, 500, 1, 0, 0, 1000), 1000);
    }

    #[test]
    fn advance_saturates_without_overflow_or_panic() {
        assert_eq!(advance(0, u64::MAX, u64::MAX, 1, 0, 1000), 1000);
    }

    #[test]
    fn advance_saturates_with_nonzero_start() {
        assert_eq!(advance(500, u64::MAX, u64::MAX, 1, 100, 5000), 5000);
    }
}
