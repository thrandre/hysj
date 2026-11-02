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

/// A time as the reader says it: "12:00 today", "19:00 yesterday",
/// "08:00 tomorrow", else with the weekday or the date ("16:00 on Monday").
pub fn when(t: &Zoned, now: &Zoned) -> String {
    let day = t.date();
    let today = now.date();
    let words = if day == today {
        "today".to_string()
    } else if Some(day) == today.yesterday().ok() {
        "yesterday".into()
    } else if Some(day) == today.tomorrow().ok() {
        "tomorrow".into()
    } else if (day - today).get_days().abs() < 7 {
        t.strftime("on %A").to_string()
    } else {
        t.strftime("on %-d %B").to_string()
    };
    format!("{} {words}", t.strftime("%H:%M"))
}

/// How long unread Digest mail stays in the latest edition.
pub const LATEST_DAYS: i64 = 7;

/// Which mail a pick shows, as (after, up to and including): 0 the latest
/// edition, everything unread until the last delivery — it adds up until it's
/// marked as read — but no further back than [`LATEST_DAYS`]; 1 everything
/// older.
pub fn window(pick: u8, now: &Zoned) -> (Option<Zoned>, Zoned) {
    let last = last_slot(now);
    let cut = last
        .checked_sub(jiff::SignedDuration::from_hours(24 * LATEST_DAYS))
        .expect("valid time");
    match pick {
        0 => (Some(cut), last),
        _ => (None, cut),
    }
}

/// What the latest edition says it covers ("since Friday 16:00"): the
/// delivery before its oldest mail, else the delivery before this one.
pub fn since(oldest: Option<i64>, slot: &Zoned) -> Zoned {
    let before = |z: &Zoned| {
        last_slot(
            &z.checked_sub(jiff::SignedDuration::from_secs(1))
                .expect("valid time"),
        )
    };
    match oldest.and_then(|t| jiff::Timestamp::from_second(t).ok()) {
        Some(t) => before(&t.to_zoned(slot.time_zone().clone())),
        None => before(slot),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> Zoned {
        format!("{s}[Europe/Oslo]").parse().unwrap()
    }

    #[test]
    fn latest_adds_up_for_a_week() {
        let now = at("2026-10-21T14:05:00+02:00");
        let (after, upto) = window(0, &now);
        assert_eq!(upto, at("2026-10-21T08:00:00+02:00"));
        assert_eq!(after, Some(at("2026-10-14T08:00:00+02:00")));
        assert_eq!(window(1, &now), (None, at("2026-10-14T08:00:00+02:00")));
        // Unread since Monday morning: "since 16:00 on Sunday".
        let oldest = at("2026-10-19T07:30:00+02:00").timestamp().as_second();
        assert_eq!(since(Some(oldest), &upto), at("2026-10-18T16:00:00+02:00"));
        assert_eq!(since(None, &upto), at("2026-10-20T16:00:00+02:00"));
    }

    #[test]
    fn when_says_the_day() {
        let now = at("2026-10-21T14:05:00+02:00");
        let w = |s: &str| when(&at(s), &now);
        assert_eq!(w("2026-10-21T12:00:00+02:00"), "12:00 today");
        assert_eq!(w("2026-10-20T19:00:00+02:00"), "19:00 yesterday");
        assert_eq!(w("2026-10-22T12:00:00+02:00"), "12:00 tomorrow");
        assert_eq!(w("2026-10-19T08:00:00+02:00"), "08:00 on Monday");
        assert_eq!(w("2026-10-01T08:00:00+02:00"), "08:00 on 1 October");
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
