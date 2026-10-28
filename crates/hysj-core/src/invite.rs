//! Calendar invites in mail (iCalendar, RFC 5545) and the answers to them
//! (iTIP REPLY, RFC 5546); events as `.ics` for the reader's own calendar.

use jiff::Timestamp;
use jiff::civil::{Date, DateTime};
use jiff::tz::TimeZone;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Invite {
    /// "REQUEST" or "CANCEL" (others aren't kept).
    pub method: String,
    pub uid: String,
    pub sequence: i64,
    pub summary: String,
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
    pub location: String,
    pub organizer_name: String,
    pub organizer_addr: String,
}

/// Lines with folded continuations joined.
fn unfold(ics: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in ics.lines() {
        match (
            line.strip_prefix(' ').or_else(|| line.strip_prefix('\t')),
            out.last_mut(),
        ) {
            (Some(rest), Some(last)) => last.push_str(rest),
            _ => out.push(line.trim_end_matches('\r').to_string()),
        }
    }
    out
}

/// A property's parameters, (NAME, value).
type Params = Vec<(String, String)>;

/// `NAME;PARAM=x;PARAM2=y:value` → (name, params, value).
fn split(line: &str) -> Option<(String, Params, String)> {
    // The value starts at the first ':' outside a quoted parameter.
    let mut quoted = false;
    let colon = line.char_indices().find(|&(_, c)| {
        if c == '"' {
            quoted = !quoted;
        }
        c == ':' && !quoted
    })?;
    let (head, value) = (&line[..colon.0], &line[colon.0 + 1..]);
    let mut parts = head.split(';');
    let name = parts.next()?.to_ascii_uppercase();
    let params = parts
        .filter_map(|p| p.split_once('='))
        .map(|(k, v)| (k.to_ascii_uppercase(), v.trim_matches('"').to_string()))
        .collect();
    Some((name, params, value.to_string()))
}

fn unescape(v: &str) -> String {
    v.replace("\\n", "\n")
        .replace("\\N", "\n")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
}

/// A DTSTART/DTEND value → (unix seconds, all day).
fn time(params: &[(String, String)], value: &str) -> Option<(i64, bool)> {
    let param = |k: &str| params.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
    if param("VALUE") == Some("DATE") || value.len() == 8 {
        let d = Date::strptime("%Y%m%d", value).ok()?;
        let z = d.to_zoned(TimeZone::system()).ok()?;
        return Some((z.timestamp().as_second(), true));
    }
    if let Some(utc) = value.strip_suffix('Z') {
        let dt = DateTime::strptime("%Y%m%dT%H%M%S", utc).ok()?;
        return Some((
            dt.to_zoned(TimeZone::UTC).ok()?.timestamp().as_second(),
            false,
        ));
    }
    let dt = DateTime::strptime("%Y%m%dT%H%M%S", value).ok()?;
    // Outlook names zones its own way ("W. Europe Standard Time"): those, and
    // floating times, are read in this Mac's zone.
    let tz = param("TZID")
        .and_then(|t| jiff::tz::db().get(t).ok())
        .unwrap_or_else(TimeZone::system);
    Some((dt.to_zoned(tz).ok()?.timestamp().as_second(), false))
}

/// The first event of an iCalendar text, if it's a request or a cancellation.
pub fn parse(ics: &str) -> Option<Invite> {
    let mut inv = Invite::default();
    let (mut in_event, mut end_seen) = (false, false);
    for line in unfold(ics) {
        let Some((name, params, value)) = split(&line) else {
            continue;
        };
        match (name.as_str(), in_event) {
            ("METHOD", false) => inv.method = value.trim().to_ascii_uppercase(),
            ("BEGIN", _) if value.eq_ignore_ascii_case("VEVENT") => in_event = true,
            ("END", true) if value.eq_ignore_ascii_case("VEVENT") => break,
            ("UID", true) => inv.uid = value.trim().to_string(),
            ("SEQUENCE", true) => inv.sequence = value.trim().parse().unwrap_or(0),
            ("SUMMARY", true) => inv.summary = unescape(&value),
            ("LOCATION", true) => inv.location = unescape(&value),
            ("DTSTART", true) => (inv.start, inv.all_day) = time(&params, value.trim())?,
            ("DTEND", true) => {
                inv.end = time(&params, value.trim())?.0;
                end_seen = true;
            }
            ("ORGANIZER", true) => {
                inv.organizer_addr = mailto(&value);
                inv.organizer_name = params
                    .iter()
                    .find(|(k, _)| k == "CN")
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default();
            }
            _ => {}
        }
    }
    if !end_seen {
        inv.end = inv.start + if inv.all_day { 86_400 } else { 3_600 };
    }
    let kept = matches!(inv.method.as_str(), "REQUEST" | "CANCEL");
    (kept && !inv.uid.is_empty() && inv.start > 0).then_some(inv)
}

fn mailto(v: &str) -> String {
    let v = v.trim();
    v.get(..7)
        .filter(|p| p.eq_ignore_ascii_case("mailto:"))
        .map_or(v, |_| &v[7..])
        .to_lowercase()
}

fn escape(v: &str) -> String {
    v.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\n', "\\n")
}

fn utc(ts: i64) -> String {
    Timestamp::from_second(ts)
        .map(|t| t.strftime("%Y%m%dT%H%M%SZ").to_string())
        .unwrap_or_default()
}

fn date(ts: i64) -> String {
    Timestamp::from_second(ts)
        .map(|t| {
            t.to_zoned(TimeZone::system())
                .strftime("%Y%m%d")
                .to_string()
        })
        .unwrap_or_default()
}

/// Lines folded at 75 octets, CRLF-terminated (RFC 5545 §3.1).
fn fold(lines: &[String]) -> String {
    let mut out = String::new();
    for l in lines {
        let mut width = 0;
        for c in l.chars() {
            if width + c.len_utf8() > 74 {
                out.push_str("\r\n ");
                width = 1;
            }
            out.push(c);
            width += c.len_utf8();
        }
        out.push_str("\r\n");
    }
    out
}

fn when(inv: &Invite) -> [String; 2] {
    if inv.all_day {
        [
            format!("DTSTART;VALUE=DATE:{}", date(inv.start)),
            format!("DTEND;VALUE=DATE:{}", date(inv.end)),
        ]
    } else {
        [
            format!("DTSTART:{}", utc(inv.start)),
            format!("DTEND:{}", utc(inv.end)),
        ]
    }
}

/// "ACCEPTED", "TENTATIVE" or "DECLINED" from `attendee`, to the organizer.
pub fn reply(inv: &Invite, attendee: &str, partstat: &str, now: i64) -> String {
    let [start, end] = when(inv);
    let lines = [
        "BEGIN:VCALENDAR".into(),
        "PRODID:-//Hysj//EN".into(),
        "VERSION:2.0".into(),
        "METHOD:REPLY".into(),
        "BEGIN:VEVENT".into(),
        format!("UID:{}", inv.uid),
        format!("SEQUENCE:{}", inv.sequence),
        format!("DTSTAMP:{}", utc(now)),
        start,
        end,
        format!("SUMMARY:{}", escape(&inv.summary)),
        format!("ORGANIZER:mailto:{}", inv.organizer_addr),
        format!("ATTENDEE;PARTSTAT={partstat}:mailto:{attendee}"),
        "END:VEVENT".into(),
        "END:VCALENDAR".into(),
    ];
    fold(&lines)
}

/// The event on its own, for the reader's calendar app to add.
pub fn event(inv: &Invite, now: i64) -> String {
    let [start, end] = when(inv);
    let mut lines = vec![
        "BEGIN:VCALENDAR".into(),
        "PRODID:-//Hysj//EN".into(),
        "VERSION:2.0".into(),
        "BEGIN:VEVENT".into(),
        format!("UID:{}", inv.uid),
        format!("DTSTAMP:{}", utc(now)),
        start,
        end,
        format!("SUMMARY:{}", escape(&inv.summary)),
    ];
    if !inv.location.is_empty() {
        lines.push(format!("LOCATION:{}", escape(&inv.location)));
    }
    lines.extend(["END:VEVENT".into(), "END:VCALENDAR".into()]);
    fold(&lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICS: &str = "BEGIN:VCALENDAR\r\nMETHOD:REQUEST\r\nBEGIN:VTIMEZONE\r\nTZID:Europe/Oslo\r\nEND:VTIMEZONE\r\nBEGIN:VEVENT\r\nUID:abc-123@example.com\r\nSEQUENCE:2\r\nSUMMARY:Sync engine design\r\n  review\r\nDTSTART;TZID=Europe/Oslo:20261029T173000\r\nDTEND;TZID=Europe/Oslo:20261029T183000\r\nLOCATION:Video call\\, room 2\r\nORGANIZER;CN=\"Berg, Marcus\":mailto:Marcus@Example.com\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    #[test]
    fn reads_a_request() {
        let i = parse(ICS).unwrap();
        assert_eq!(i.summary, "Sync engine design review");
        assert_eq!(i.location, "Video call, room 2");
        assert_eq!(
            (i.organizer_name.as_str(), i.organizer_addr.as_str()),
            ("Berg, Marcus", "marcus@example.com")
        );
        // 17:30 in Oslo (CET, UTC+1 on 29 October 2026) is 16:30 UTC.
        assert_eq!(utc(i.start), "20261029T163000Z");
        assert_eq!(i.end - i.start, 3600);
        assert_eq!(i.sequence, 2);
        assert!(parse(&ICS.replace("METHOD:REQUEST", "METHOD:PUBLISH")).is_none());
        let all_day = parse(
            &ICS.replace(
                "DTSTART;TZID=Europe/Oslo:20261029T173000",
                "DTSTART;VALUE=DATE:20261029",
            )
            .replace("DTEND;TZID=Europe/Oslo:20261029T183000\r\n", ""),
        )
        .unwrap();
        assert!(all_day.all_day);
        assert_eq!(all_day.end - all_day.start, 86_400);
    }

    #[test]
    fn found_in_mail() {
        assert_eq!(
            crate::extract::candidate("Invitation: Design review @ Thu 29 Oct"),
            Some(crate::extract::Candidate::Structured)
        );
        let raw = format!(
            "From: Marcus <marcus@example.com>\r\nSubject: Invitation: Design review\r\nContent-Type: multipart/alternative; boundary=b\r\n\r\n--b\r\nContent-Type: text/plain\r\n\r\nJoin us\r\n--b\r\nContent-Type: text/calendar; method=REQUEST; charset=UTF-8\r\n\r\n{ICS}--b--\r\n"
        );
        let found = crate::extract::extract(raw.as_bytes());
        assert!(
            matches!(found.as_slice(), [crate::extract::Extract::Invite(i)] if i.uid == "abc-123@example.com")
        );
    }

    #[test]
    fn writes_a_reply() {
        let i = parse(ICS).unwrap();
        let r = reply(&i, "kai@example.com", "ACCEPTED", 0);
        assert!(r.contains("METHOD:REPLY\r\n"));
        assert!(r.contains("ATTENDEE;PARTSTAT=ACCEPTED:mailto:kai@example.com\r\n"));
        assert!(r.contains("UID:abc-123@example.com\r\nSEQUENCE:2\r\n"));
        assert!(r.lines().all(|l| l.len() <= 75));
        assert_eq!(
            parse(&r.replace("METHOD:REPLY", "METHOD:REQUEST")).map(|p| p.uid),
            Some(i.uid.clone())
        );
        assert!(event(&i, 0).contains("LOCATION:Video call\\, room 2\r\n"));
    }
}
