//! Waiting on others: mail you sent that asked for something, and when an
//! answer is due — from your own words ("by Friday"), else three working days.
//! Rules only; what you asked is shown as you wrote it.

use jiff::civil::{Time, Weekday};
use jiff::{ToSpan, Zoned};

const REQUESTS: &[&str] = &[
    "could you",
    "can you",
    "would you",
    "will you",
    "please",
    "let me know",
    "send me",
    "kan du",
    "kunne du",
    "vil du",
    "gi beskjed",
    "send meg",
    "si ifra",
    "si fra",
];

/// Your own text of a sent mail: quoted replies and the signature cut off.
fn own_text(body: &str) -> String {
    let mut out = Vec::new();
    for line in body.lines() {
        let t = line.trim();
        let lower = t.to_lowercase();
        if t == "--"
            || t.starts_with("-----")
            || ((lower.starts_with("on ") || lower.starts_with("den "))
                && (lower.ends_with("wrote:") || lower.ends_with("skrev:")))
        {
            break;
        }
        if !t.starts_with('>') {
            out.push(t);
        }
    }
    out.join(" ")
}

/// The sentence where you asked for something, as you wrote it.
pub fn asked(body: &str) -> Option<String> {
    let text = own_text(body);
    let mut start = 0;
    let mut sentences = Vec::new();
    for (i, c) in text.char_indices() {
        if matches!(c, '.' | '?' | '!') {
            sentences.push(&text[start..=i]);
            start = i + c.len_utf8();
        }
    }
    sentences.push(&text[start..]);
    sentences
        .into_iter()
        .map(str::trim)
        .filter(|s| s.chars().count() >= 8)
        .find(|s| {
            let l = s.to_lowercase();
            s.ends_with('?') || REQUESTS.iter().any(|r| l.contains(r))
        })
        .map(|s| {
            if s.chars().count() > 200 {
                format!("{}…", s.chars().take(199).collect::<String>().trim_end())
            } else {
                s.to_string()
            }
        })
}

const DAYS: &[(&str, Weekday)] = &[
    ("monday", Weekday::Monday),
    ("tuesday", Weekday::Tuesday),
    ("wednesday", Weekday::Wednesday),
    ("thursday", Weekday::Thursday),
    ("friday", Weekday::Friday),
    ("saturday", Weekday::Saturday),
    ("sunday", Weekday::Sunday),
    ("mandag", Weekday::Monday),
    ("tirsdag", Weekday::Tuesday),
    ("onsdag", Weekday::Wednesday),
    ("torsdag", Weekday::Thursday),
    ("fredag", Weekday::Friday),
    ("lørdag", Weekday::Saturday),
    ("søndag", Weekday::Sunday),
];

/// When an answer is due, and your words if you set the date ("by Friday").
pub fn due(body: &str, sent: &Zoned) -> (Zoned, Option<String>) {
    let text = own_text(body);
    let lower = text.to_lowercase();
    let end_of = |d: Zoned| {
        d.with()
            .time(Time::constant(17, 0, 0, 0))
            .build()
            .unwrap_or(d)
    };
    for by in ["by ", "before ", "innen ", "før "] {
        for (at, _) in lower.match_indices(by) {
            // Only at a word's start ("standby " isn't "by ").
            if at > 0
                && lower[..at]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_alphanumeric)
            {
                continue;
            }
            let rest = &lower[at + by.len()..];
            // As you wrote it; `lower` and `text` line up except for rare
            // letters whose lower case is longer (then the lower case).
            let words = |len: usize| {
                text.get(at..at + by.len() + len)
                    .unwrap_or(&lower[at..at + by.len() + len])
                    .to_string()
            };
            if let Some((name, day)) = DAYS.iter().find(|(n, _)| rest.starts_with(n)) {
                let mut d = sent.tomorrow().unwrap_or_else(|_| sent.clone());
                while d.weekday() != *day {
                    d = d.tomorrow().unwrap_or(d);
                }
                return (end_of(d), Some(words(name.len())));
            }
            for (w, n) in [
                ("tomorrow", 1),
                ("i morgen", 1),
                ("end of day", 0),
                ("the end of the day", 0),
            ] {
                if rest.starts_with(w) {
                    let d = sent.checked_add(n.days()).unwrap_or_else(|_| sent.clone());
                    return (end_of(d), Some(words(w.len())));
                }
            }
        }
    }
    let mut d = sent.clone();
    let mut left = 3;
    while left > 0 {
        d = d.tomorrow().unwrap_or(d);
        if !matches!(d.weekday(), Weekday::Saturday | Weekday::Sunday) {
            left -= 1;
        }
    }
    (d, None)
}

/// "Re: Fwd: Budget" → "budget", for matching a reply to what you sent.
pub fn bare_subject(s: &str) -> String {
    let mut s = s.trim();
    loop {
        let l = s.to_lowercase();
        match ["re:", "sv:", "aw:", "fwd:", "fw:", "vs:"]
            .iter()
            .find(|p| l.starts_with(*p))
        {
            Some(p) => s = s[p.len()..].trim_start(),
            None => return l,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_you_asked() {
        let body = "Hi Ingrid,\n\nThanks for the minutes. Could you send me the latest budget draft by Friday? I’d like to read it first.\n\n--\nKai\n\nOn Mon, Ingrid wrote:\n> Can you come?";
        assert_eq!(
            asked(body).as_deref(),
            Some("Could you send me the latest budget draft by Friday?")
        );
        assert_eq!(asked("Thanks, see you then."), None);
        assert_eq!(asked("> Can you come?\nSounds good."), None);
    }

    #[test]
    fn deadlines() {
        // A Thursday afternoon.
        let sent: Zoned = "2026-10-22T15:00:00+02:00[Europe/Oslo]".parse().unwrap();
        let (d, w) = due("Could you send it by Friday?", &sent);
        assert_eq!(
            (d.date().to_string(), w.as_deref()),
            ("2026-10-23".into(), Some("by Friday"))
        );
        let (d, w) = due("Kan du svare innen mandag?", &sent);
        assert_eq!(
            (d.date().to_string(), w.as_deref()),
            ("2026-10-26".into(), Some("innen mandag"))
        );
        // Three working days, skipping the weekend.
        let (d, w) = due("Could you have a look?", &sent);
        assert_eq!((d.date().to_string(), w), ("2026-10-27".into(), None));
        assert_eq!(due("on standby friday?", &sent).1, None);
        assert_eq!(bare_subject("SV: Re: Budget"), "budget");
    }
}
