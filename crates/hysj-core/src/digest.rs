//! Digest delivery times. Mail for the Digest is held between deliveries and
//! shown at the next one. The reader picks the hours (08:00 and 16:00 until
//! they do); the daemon sets them at start and when they change.

use std::sync::RwLock;

use jiff::{ToSpan, Zoned, civil::Time};

/// ponytail: one process-wide schedule (one reader per daemon).
static HOURS: RwLock<Vec<i8>> = RwLock::new(Vec::new());
pub const DEFAULT_HOURS: [i8; 2] = [8, 16];

/// Sets the delivery hours (whole hours, any order; empty: the default).
pub fn set_hours(hours: &[i8]) {
    let mut h: Vec<i8> = hours
        .iter()
        .copied()
        .filter(|h| (0..24).contains(h))
        .collect();
    h.sort_unstable();
    h.dedup();
    *HOURS.write().expect("not poisoned") = h;
}

/// The delivery hours, sorted.
pub fn hours() -> Vec<i8> {
    let h = HOURS.read().expect("not poisoned").clone();
    if h.is_empty() {
        DEFAULT_HOURS.to_vec()
    } else {
        h
    }
}

fn times_of(hours: &[i8]) -> Vec<Time> {
    hours
        .iter()
        .copied()
        .map(|h| Time::new(h, 0, 0, 0).expect("an hour of the day"))
        .collect()
}

/// The most recent delivery at or before `now`.
pub fn last_slot(now: &Zoned) -> Zoned {
    last_slot_at(now, &hours())
}

fn last_slot_at(now: &Zoned, hours: &[i8]) -> Zoned {
    let today = now.date();
    let mut best = None;
    for day in [today, today.yesterday().expect("valid date")] {
        for t in times_of(hours) {
            let at = day
                .to_datetime(t)
                .to_zoned(now.time_zone().clone())
                .expect("valid time");
            if &at <= now && best.as_ref().is_none_or(|b: &Zoned| &at > b) {
                best = Some(at);
            }
        }
    }
    best.expect("a slot within the last day")
}

/// The next delivery after `now`.
pub fn next_slot(now: &Zoned) -> Zoned {
    next_slot_at(now, &hours())
}

fn next_slot_at(now: &Zoned, hours: &[i8]) -> Zoned {
    let last = last_slot_at(now, hours);
    let times = times_of(hours);
    let later_today = times
        .iter()
        .map(|t| {
            last.date()
                .to_datetime(*t)
                .to_zoned(now.time_zone().clone())
                .expect("valid time")
        })
        .find(|z| *z > last);
    later_today.unwrap_or_else(|| {
        (last.date() + 1.day())
            .to_datetime(times[0])
            .to_zoned(now.time_zone().clone())
            .expect("valid time")
    })
}

/// "Morning" for the 08:00 delivery, "Afternoon" for 16:00, and so on.
pub fn name(slot: &Zoned) -> &'static str {
    match slot.hour() {
        0..11 => "Morning",
        11..14 => "Midday",
        14..18 => "Afternoon",
        _ => "Evening",
    }
}

/// The latest delivery and the three before it, newest first.
pub fn slots(now: &Zoned) -> [Zoned; 4] {
    let before = |z: &Zoned| {
        last_slot(
            &z.checked_sub(jiff::SignedDuration::from_secs(1))
                .expect("valid time"),
        )
    };
    let s0 = last_slot(now);
    let s1 = before(&s0);
    let s2 = before(&s1);
    let s3 = before(&s2);
    [s0, s1, s2, s3]
}

/// A pick's delivery and the one before it: 0 the latest, 1–2 earlier, 3
/// everything older (then the "previous" is the start of time).
pub fn window(pick: u8, now: &Zoned) -> (Option<Zoned>, Zoned) {
    let s = slots(now);
    match pick {
        0 => (Some(s[1].clone()), s[0].clone()),
        1 => (Some(s[2].clone()), s[1].clone()),
        2 => (Some(s[3].clone()), s[2].clone()),
        _ => (None, s[3].clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> Zoned {
        format!("{s}[Europe/Oslo]").parse().unwrap()
    }

    #[test]
    fn slots() {
        assert_eq!(
            last_slot(&at("2026-10-21T14:05:00+02:00")),
            at("2026-10-21T08:00:00+02:00")
        );
        assert_eq!(
            next_slot(&at("2026-10-21T14:05:00+02:00")),
            at("2026-10-21T16:00:00+02:00")
        );
        assert_eq!(
            last_slot(&at("2026-10-21T07:59:00+02:00")),
            at("2026-10-20T16:00:00+02:00")
        );
        assert_eq!(
            next_slot(&at("2026-10-21T17:00:00+02:00")),
            at("2026-10-22T08:00:00+02:00")
        );
        assert_eq!(
            last_slot(&at("2026-10-21T16:00:00+02:00")),
            at("2026-10-21T16:00:00+02:00")
        );
        assert_eq!(name(&at("2026-10-21T16:00:00+02:00")), "Afternoon");
    }

    #[test]
    fn chosen_hours() {
        let h = [7, 12, 19];
        let t = at("2026-10-21T13:00:00+02:00");
        assert_eq!(last_slot_at(&t, &h), at("2026-10-21T12:00:00+02:00"));
        assert_eq!(next_slot_at(&t, &h), at("2026-10-21T19:00:00+02:00"));
        let late = at("2026-10-21T20:00:00+02:00");
        assert_eq!(next_slot_at(&late, &h), at("2026-10-22T07:00:00+02:00"));
        assert_eq!(
            next_slot_at(&late, &[16]),
            at("2026-10-22T16:00:00+02:00"),
            "once a day"
        );
    }
}
