//! Rules-tier extraction from message bodies: one-time codes, password-reset
//! links, and schema.org JSON-LD bookings / orders. No model involved.
//!
//! ponytail: schema.org microdata and mails without structured data are left to
//! the AI tier (step 9).

use mail_parser::{Message, MessageParser, MimeHeaders};
use serde_json::Value;

/// Which extractors a message is worth fetching a body for, from its headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Candidate {
    /// Fetched first: codes go stale in minutes.
    Code,
    Structured,
    /// A sender with its own reader (FINN alerts → listings).
    Source,
}

/// Senders whose mail has a reader of its own (see [`crate::finn`]).
pub fn source(from_addr: &str) -> bool {
    let d = from_addr
        .rsplit_once('@')
        .map(|(_, d)| d.to_ascii_lowercase())
        .unwrap_or_default();
    // Saved-search alerts only; FINN's curated mail (inspirasjon.finn.no) has
    // opaque links and reads as a newsletter.
    d == "finn.no"
}

const CODE_SUBJECT: &[&str] = &[
    "code",
    "kode",
    "verif*",
    "sign in",
    "sign-in",
    "logg inn",
    "login",
    "log in",
    "engangskode",
    "one-time",
    "passcode",
    "sikkerhetskode",
    "engångskod",
    "inloggning",
    "otp",
    "reset",
    "tilbakestill",
    "nullstill",
    "passord",
    "password",
    "återställ",
    "lösenord",
    "magic link",
    "bekreft e-post",
    "bekreft epost",
    "bekräfta",
    "confirm your email",
];
const STRUCTURED_SUBJECT: &[&str] = &[
    // Calendar invites (their text/calendar part); Outlook's carry only the
    // meeting's title. ponytail: by subject; a Content-Class header check if
    // Outlook invites matter.
    "invitation",
    "invitasjon",
    "invite",
    "updated event",
    "oppdatert",
    "canceled event",
    "cancelled event",
    "avlyst",
    "order",
    "ordre",
    "bestilling",
    "kvittering",
    "receipt",
    "takk for kjøpet",
    "kjøp",
    "shipped",
    "sendt",
    "levert",
    "delivered",
    "pakke",
    "package",
    "parcel",
    "sporing",
    "tracking",
    "on its way",
    "på vei",
    "beställning",
    "kvitto",
    "booking",
    "bokning",
    "reservation",
    "reservasjon",
    "bekreftelse",
    "bekräftelse",
    "confirmation",
    "flight",
    "fly",
    "hotel",
    "hotell",
    "billett",
    "ticket",
    "check-in",
    "itinerary",
    "reiserute",
    "boarding",
];
/// "Code" words that are marketing, not authentication.
const NOT_AUTH: &[&str] = &[
    "rabattkode",
    "rabattkod",
    "discount code",
    "promo code",
    "promokode",
    "kampanjekode",
    "gift code",
    "gavekode",
    "voucher",
    "coupon",
    "kupong",
    "referral code",
    "vervekode",
];

/// Bumped when the code rules change so that mail they once took for a
/// code isn't one: the store forgets those codes (see `Store::init`).
pub const CODE_RULES: u32 = 2;

/// Endings a code word may take and still be that word: "codes", "koden",
/// "passordet", "verified". Anything longer is another word ("CodePen").
const INFLECTIONS: &[&str] = &["", "s", "n", "en", "et", "ne", "ene", "er", "d", "ed"];

/// Whether `text` (lower case) has `term` as a word: starting on a word
/// boundary and ending in at most an inflection. A term ending in `*` is a
/// prefix ("verif*": verify, verifisering). Nordic code words also end
/// compounds ("innloggingskode", "verifieringskod").
fn has_word(text: &str, term: &str) -> bool {
    let (term, prefix) = match term.strip_suffix('*') {
        Some(t) => (t, true),
        None => (term, false),
    };
    let compound = matches!(term, "kode" | "kod");
    text.match_indices(term).any(|(i, _)| {
        let starts = compound
            || text[..i]
                .chars()
                .next_back()
                .is_none_or(|c| !c.is_alphanumeric());
        let rest: String = text[i + term.len()..]
            .chars()
            .take_while(|c| c.is_alphanumeric())
            .collect();
        starts && (prefix || INFLECTIONS.contains(&rest.as_str()))
    })
}

pub fn candidate(subject: &str) -> Option<Candidate> {
    let s = subject.to_lowercase();
    if NOT_AUTH.iter().any(|w| s.contains(w)) {
        return STRUCTURED_SUBJECT
            .iter()
            .any(|w| s.contains(w))
            .then_some(Candidate::Structured);
    }
    if CODE_SUBJECT.iter().any(|w| has_word(&s, w)) {
        Some(Candidate::Code)
    } else if STRUCTURED_SUBJECT.iter().any(|w| s.contains(w)) {
        Some(Candidate::Structured)
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Extract {
    Code(Code),
    Booking(Booking),
    Order(Order),
    Listing(crate::finn::Listing),
    Invite(crate::invite::Invite),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeKind {
    SignIn,
    Verification,
    Code,
    Reset,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Code {
    pub kind: CodeKind,
    /// Sender's display name, else domain.
    pub service: String,
    /// Digits only. `None` for reset links.
    pub code: Option<String>,
    pub link: Option<String>,
    /// Host the reset link points to, and whether it is the sender's own domain.
    pub link_host: Option<String>,
    pub link_ok: bool,
    pub sender_domain: String,
    /// How long it is valid, as stated in the mail or a default.
    pub valid_minutes: u32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Booking {
    /// "flight", "train", "bus", "hotel", "table", "event".
    pub kind: String,
    /// Merge key: kind, reference and (for legs sharing one PNR) the flight number.
    pub key: String,
    pub reference: String,
    pub title: String,
    /// ISO 8601 as written in the mail (often local time without offset).
    pub start: Option<String>,
    pub end: Option<String>,
    /// Airline / hotel brand / operator.
    pub provider: Option<String>,
    /// Destination city, for naming trips.
    pub city: Option<String>,
    pub seat: Option<String>,
    pub party: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Order {
    /// Merge key: merchant + order number.
    pub key: String,
    pub store: String,
    pub number: String,
    pub ordered: Option<String>,
    pub items: Vec<String>,
    pub total: Option<String>,
    /// 1 Ordered, 2 Shipped, 3 Out for delivery, 4 Delivered.
    pub status: u8,
    pub eta: Option<String>,
    pub tracking_url: Option<String>,
    pub return_days: Option<u32>,
}

/// Everything extractable from a raw message (possibly truncated).
pub fn extract(raw: &[u8]) -> Vec<Extract> {
    let Some(msg) = MessageParser::default().parse(raw) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let subject = msg.subject().unwrap_or_default().to_string();
    let text = body_text(&msg);
    let html = msg.body_html(0).map(|h| h.into_owned()).unwrap_or_default();
    let from = msg.from().and_then(|f| f.first());
    let from_addr = from
        .and_then(|a| a.address())
        .unwrap_or_default()
        .to_lowercase();
    let sender_domain = from_addr
        .rsplit_once('@')
        .map(|(_, d)| d.to_string())
        .unwrap_or_default();
    let service = from
        .and_then(|a| a.name())
        .map(str::to_string)
        .unwrap_or_else(|| sender_domain.clone());

    if source(&from_addr) {
        out.extend(
            crate::finn::listings(&html)
                .into_iter()
                .map(Extract::Listing),
        );
        return out;
    }
    if candidate(&subject) == Some(Candidate::Code) && authenticated(raw, &sender_domain) {
        out.extend(find_code(&subject, &text, &html, &service, &sender_domain).map(Extract::Code));
    }
    for v in json_ld(&html) {
        walk(&v, &mut out);
    }
    // A calendar invite: its text/calendar part, or an attached .ics.
    let ics = msg.parts.iter().find_map(|p| {
        let calendar = p.is_content_type("text", "calendar");
        let attached = p
            .attachment_name()
            .is_some_and(|n| n.to_lowercase().ends_with(".ics"));
        (calendar || attached).then(|| String::from_utf8_lossy(p.contents()).into_owned())
    });
    if let Some(i) = ics.as_deref().and_then(crate::invite::parse) {
        out.push(Extract::Invite(i));
    }
    out
}

fn body_text(msg: &Message) -> String {
    msg.body_text(0).map(|t| t.into_owned()).unwrap_or_default()
}

// --- Codes ----------------------------------------------------------------------

/// Trust only the topmost Authentication-Results, written by our own receiving
/// server; senders can add their own further down.
/// ponytail: relies on the provider's verdict (Gmail adds it); verify DKIM
/// ourselves with mail-auth for servers that don't.
pub fn authenticated(raw: &[u8], domain: &str) -> bool {
    let Some(ar) = topmost_header(raw, "authentication-results") else {
        return false;
    };
    let ar = ar.to_lowercase();
    let org = |d: &str| {
        let labels: Vec<&str> = d
            .trim_matches(|c: char| c == '@' || c.is_whitespace())
            .rsplit('.')
            .take(2)
            .collect();
        labels.into_iter().rev().collect::<Vec<_>>().join(".")
    };
    let mine = org(domain);
    let aligned = |d: &str| !mine.is_empty() && org(d) == mine;
    ar.split(';').map(str::trim).any(|clause| {
        let value = |key: &str| {
            clause
                .split_whitespace()
                .find_map(|w| w.strip_prefix(key))
                .map(|v| {
                    v.trim_matches(|c: char| c == '"' || c == ')' || c == '(')
                        .to_string()
                })
        };
        (clause.starts_with("dmarc=pass") && value("header.from=").is_some_and(|d| aligned(&d)))
            || (clause.starts_with("dkim=pass")
                && (value("header.d=").is_some_and(|d| aligned(&d))
                    || value("header.i=")
                        .is_some_and(|i| aligned(i.rsplit('@').next().unwrap_or(&i)))))
    })
}

/// The first header with this (lower-case) name, unfolded. mail-parser's
/// lookup returns the last one, which a sender can supply.
fn topmost_header(raw: &[u8], name: &str) -> Option<String> {
    let head = String::from_utf8_lossy(raw);
    let mut found: Option<String> = None;
    for line in head.split('\n').map(|l| l.trim_end_matches('\r')) {
        if line.is_empty() {
            break;
        }
        match &mut found {
            Some(v) if line.starts_with([' ', '\t']) => v.push_str(line),
            Some(_) => break,
            None => {
                if let Some((n, v)) = line.split_once(':')
                    && n.eq_ignore_ascii_case(name)
                {
                    found = Some(v.to_string());
                }
            }
        }
    }
    found
}

const CODE_WORDS: &[&str] = &[
    "code",
    "kode",
    "koden",
    "passcode",
    "engangskode",
    "sikkerhetskode",
    "pin",
    "otp",
    "engångskod",
    "verifieringskod",
    "verification",
    "verifisering",
    "bekreftelseskode",
    "security",
];

fn find_code(subject: &str, text: &str, html: &str, service: &str, domain: &str) -> Option<Code> {
    let s = subject.to_lowercase();
    let body = if text.trim().is_empty() {
        strip_tags(html)
    } else {
        text.to_string()
    };
    let body: String = body.chars().take(4000).collect();
    let lower = format!("{s}\n{}", body.to_lowercase());
    let kind = if ["reset", "tilbakestill", "nullstill", "återställ"]
        .iter()
        .any(|w| s.contains(w))
        || (["passord", "password", "lösenord"]
            .iter()
            .any(|w| s.contains(w))
            && !lower.contains("kode")
            && !lower.contains("code"))
    {
        CodeKind::Reset
    } else if [
        "sign in",
        "sign-in",
        "logg inn",
        "login",
        "log in",
        "innlogging",
        "inloggning",
    ]
    .iter()
    .any(|w| lower.contains(w))
    {
        CodeKind::SignIn
    } else if ["verif", "bekreft", "confirm"]
        .iter()
        .any(|w| lower.contains(w))
    {
        CodeKind::Verification
    } else {
        CodeKind::Code
    };
    let valid = stated_minutes(&lower);

    if kind == CodeKind::Reset {
        let link = links(html, &body).into_iter().find(|l| {
            let l = l.to_lowercase();
            [
                "reset",
                "password",
                "passord",
                "token",
                "recover",
                "tilbakestill",
                "nullstill",
            ]
            .iter()
            .any(|w| l.contains(w))
        })?;
        let host = link
            .split("://")
            .nth(1)
            .and_then(|r| r.split(['/', '?', '#']).next())
            .unwrap_or_default()
            .to_lowercase();
        let ok = !domain.is_empty()
            && (host == domain
                || host.ends_with(&format!(".{domain}"))
                || domain.ends_with(&format!(".{host}")));
        return Some(Code {
            kind,
            service: service.into(),
            code: None,
            link: Some(link),
            link_host: Some(host),
            link_ok: ok,
            sender_domain: domain.into(),
            valid_minutes: valid.unwrap_or(60),
        });
    }

    let digits = code_token(&format!("{subject}\n{body}"))?;
    Some(Code {
        kind,
        service: service.into(),
        code: Some(digits),
        link: None,
        link_host: None,
        link_ok: false,
        sender_domain: domain.into(),
        valid_minutes: valid.unwrap_or(10),
    })
}

/// The first 4–8 digit code (`482913`, `482 913`, `4829-1305`) with a code word
/// shortly before it, that isn't a price, date, time, postcode, phone number or
/// part of a longer token.
fn code_token(text: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit()
            || (i > 0 && (chars[i - 1].is_alphanumeric() || "+-./:,€$£#".contains(chars[i - 1])))
        {
            i += 1;
            continue;
        }
        // Digits, allowing a space or hyphen between groups of 3–4
        // ("482 913", "4829-1305"), never between a date's 4-2-2.
        let start = i;
        let mut digits = String::new();
        let mut group = 0;
        let mut j = i;
        while j < chars.len() {
            if chars[j].is_ascii_digit() {
                digits.push(chars[j]);
                group += 1;
                j += 1;
            } else if (chars[j] == ' ' || chars[j] == '-')
                && (3..=4).contains(&group)
                && chars[j + 1..]
                    .iter()
                    .take_while(|c| c.is_ascii_digit())
                    .count()
                    .clamp(0, 5)
                    >= 3
                && chars[j + 1..]
                    .iter()
                    .take_while(|c| c.is_ascii_digit())
                    .count()
                    <= 4
            {
                group = 0;
                j += 1;
            } else {
                break;
            }
        }
        i = j.max(i + 1);
        let next = chars.get(j).copied().unwrap_or(' ');
        if !(4..=8).contains(&digits.len())
            || next.is_alphanumeric()
            || ".,:/%".contains(next) && chars.get(j + 1).is_some_and(|c| c.is_ascii_digit())
        {
            continue;
        }
        let before: String = chars[start.saturating_sub(60)..start]
            .iter()
            .collect::<String>()
            .to_lowercase();
        let after: String = chars[j..(j + 12).min(chars.len())]
            .iter()
            .collect::<String>()
            .to_lowercase();
        let after = after.trim_start();
        if ["kr", "nok", "sek", "eur", "usd", ",-", "%"]
            .iter()
            .any(|w| after.starts_with(w))
            || [
                "kr ", "nok ", "sek ", "kr.", "$", "€", "£", "tlf", "telefon", "phone", "+47",
                "+46", "ordre", "order", "faktura", "invoice",
            ]
            .iter()
            .any(|w| before.trim_end().ends_with(w.trim_end()))
        {
            continue;
        }
        let close = before.trim_end();
        let last = close
            .rsplit(|c: char| !c.is_alphanumeric())
            .next()
            .unwrap_or_default();
        let introduced = close.ends_with(':')
            || CODE_WORDS.iter().any(|w| has_word(last, w))
            || close.ends_with(" is")
            || close.ends_with(" er");
        // "1607 Fredrikstad": a postcode followed by a place name.
        let capital_next = chars[j..]
            .iter()
            .find(|c| !c.is_whitespace())
            .is_some_and(|c| c.is_uppercase());
        if digits.len() == 4 && capital_next && !introduced {
            continue;
        }
        let word = CODE_WORDS.iter().any(|w| has_word(&before, w))
            && !NOT_AUTH.iter().any(|w| before.contains(w));
        let year = digits.len() == 4 && (digits.starts_with("19") || digits.starts_with("20"));
        // A year only counts when the mail introduces it as the code.
        if word && (!year || introduced) {
            return Some(digits);
        }
    }
    None
}

/// "expires in 15 minutes", "gyldig i 10 minutter", "valid for 1 hour".
fn stated_minutes(lower: &str) -> Option<u32> {
    let words: Vec<&str> = lower
        .split(|c: char| c.is_whitespace() || c == '(' || c == ')')
        .filter(|w| !w.is_empty())
        .collect();
    for (i, w) in words.iter().enumerate() {
        let Ok(n) = w.parse::<u32>() else { continue };
        let unit = words.get(i + 1).copied().unwrap_or("");
        let context = words[i.saturating_sub(5)..i].join(" ");
        if ![
            "expire", "utløper", "gyldig", "valid", "går ut", "giltig", "within", "innen",
        ]
        .iter()
        .any(|k| context.contains(k))
        {
            continue;
        }
        if unit.starts_with("min") {
            return Some(n);
        }
        if ["hour", "time", "timer", "timme", "timmar"]
            .iter()
            .any(|u| unit.starts_with(u))
        {
            return Some(n * 60);
        }
    }
    None
}

fn links(html: &str, text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in html.split("href=").skip(1) {
        let q = part.chars().next().unwrap_or('"');
        if (q == '"' || q == '\'')
            && let Some(url) = part[1..].split(q).next()
        {
            out.push(unescape(url));
        }
    }
    for w in text.split_whitespace() {
        if w.starts_with("https://") || w.starts_with("http://") {
            out.push(w.trim_end_matches(['>', ')', '.', ',']).to_string());
        }
    }
    out
}

fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    unescape(&out)
}

fn unescape(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&#34;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
}

// --- schema.org JSON-LD -------------------------------------------------------------

/// Every `<script type="application/ld+json">` block, parsed.
fn json_ld(html: &str) -> Vec<Value> {
    let lower = html.to_lowercase();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(pos) = lower[from..].find("<script") {
        let tag_start = from + pos;
        let Some(tag_end) = lower[tag_start..].find('>').map(|e| tag_start + e + 1) else {
            break;
        };
        let Some(close) = lower[tag_end..].find("</script").map(|c| tag_end + c) else {
            break;
        };
        if lower[tag_start..tag_end].contains("ld+json") {
            let body = html[tag_end..close].trim();
            let parsed =
                serde_json::from_str(body).or_else(|_| serde_json::from_str(&unescape(body)));
            if let Ok(v) = parsed {
                out.push(v);
            }
        }
        from = close;
    }
    out
}

fn types(v: &Value) -> Vec<String> {
    let strip = |t: &str| t.rsplit('/').next().unwrap_or(t).to_string();
    match v.get("@type") {
        Some(Value::String(t)) => vec![strip(t)],
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(strip).collect(),
        _ => Vec::new(),
    }
}

fn walk(v: &Value, out: &mut Vec<Extract>) {
    match v {
        Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
        Value::Object(o) => {
            if let Some(g) = o.get("@graph") {
                walk(g, out);
            }
            let t = types(v);
            let is = |name: &str| t.iter().any(|x| x == name);
            if t.iter().any(|x| x.ends_with("Reservation")) {
                out.extend(booking(v).map(Extract::Booking));
            } else if is("Order") {
                out.extend(order(v).map(Extract::Order));
            } else if is("ParcelDelivery") {
                out.extend(parcel(v).map(Extract::Order));
            }
        }
        _ => {}
    }
}

fn s(v: &Value, path: &[&str]) -> Option<String> {
    let mut cur = v;
    for p in path {
        cur = match cur {
            Value::Array(a) => a.first()?.get(p)?,
            _ => cur.get(p)?,
        };
    }
    match cur {
        Value::String(x) if !x.trim().is_empty() => Some(x.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn place(v: &Value, key: &str) -> (Option<String>, Option<String>) {
    let code = s(v, &[key, "iataCode"]);
    let city = s(v, &[key, "address", "addressLocality"]).or_else(|| s(v, &[key, "name"]));
    (code, city)
}

fn booking(v: &Value) -> Option<Booking> {
    let t = types(v).into_iter().find(|x| x.ends_with("Reservation"))?;
    let reference = s(v, &["reservationNumber"]).or_else(|| s(v, &["reservationId"]))?;
    let f = v.get("reservationFor")?;
    let mut b = Booking {
        reference: reference.clone(),
        ..Default::default()
    };
    match t.as_str() {
        "FlightReservation" | "TrainReservation" | "BusReservation" => {
            let (dep, arr) = match t.as_str() {
                "FlightReservation" => ("departureAirport", "arrivalAirport"),
                "TrainReservation" => ("departureStation", "arrivalStation"),
                _ => ("departureBusStop", "arrivalBusStop"),
            };
            let (dc, dcity) = place(f, dep);
            let (ac, acity) = place(f, arr);
            let side = |city: Option<String>, code: Option<String>| match (city, code) {
                (Some(c), Some(k)) if c != k => format!("{c} {k}"),
                (Some(c), _) | (None, Some(c)) => c,
                _ => "?".into(),
            };
            b.title = format!("{} → {}", side(dcity, dc), side(acity.clone(), ac));
            b.city = acity;
            b.kind = match t.as_str() {
                "FlightReservation" => "flight",
                "TrainReservation" => "train",
                _ => "bus",
            }
            .into();
            b.start = s(f, &["departureTime"]);
            b.end = s(f, &["arrivalTime"]);
            b.provider = s(f, &["airline", "name"]).or_else(|| s(f, &["provider", "name"]));
            let number = [
                s(f, &["airline", "iataCode"]),
                s(f, &["flightNumber"]).or_else(|| s(f, &["trainNumber"])),
            ]
            .into_iter()
            .flatten()
            .collect::<String>();
            b.seat = s(v, &["reservedTicket", "ticketedSeat", "seatNumber"])
                .or_else(|| s(v, &["airplaneSeat"]));
            b.key = format!("{}:{reference}:{number}", b.kind);
        }
        "LodgingReservation" => {
            b.kind = "hotel".into();
            b.title = s(f, &["name"])?;
            b.city = s(f, &["address", "addressLocality"]);
            b.start = s(v, &["checkinTime"]).or_else(|| s(v, &["checkinDate"]));
            b.end = s(v, &["checkoutTime"]).or_else(|| s(v, &["checkoutDate"]));
            b.provider = s(v, &["provider", "name"]);
            b.key = format!("hotel:{reference}");
        }
        "FoodEstablishmentReservation" => {
            b.kind = "table".into();
            b.title = s(f, &["name"])?;
            b.city = s(f, &["address", "addressLocality"]);
            b.start = s(v, &["startTime"]);
            b.party = s(v, &["partySize"]).and_then(|p| p.parse().ok());
            b.key = format!("table:{reference}");
        }
        _ => {
            b.kind = "event".into();
            b.title = s(f, &["name"])?;
            b.city = s(f, &["location", "address", "addressLocality"])
                .or_else(|| s(f, &["location", "name"]));
            b.start = s(f, &["startDate"]).or_else(|| s(v, &["startTime"]));
            b.key = format!("event:{reference}");
        }
    }
    Some(b)
}

fn status(v: &Value) -> u8 {
    let st = s(v, &["orderStatus"])
        .or_else(|| s(v, &["deliveryStatus"]))
        .unwrap_or_default()
        .to_lowercase();
    if st.contains("delivered") {
        4
    } else if st.contains("outfordelivery") || st.contains("out for delivery") {
        3
    } else if st.contains("transit") || st.contains("pickupavailable") || st.contains("shipped") {
        2
    } else {
        1
    }
}

fn merchant(v: &Value) -> Option<String> {
    s(v, &["merchant", "name"])
        .or_else(|| s(v, &["seller", "name"]))
        .or_else(|| s(v, &["broker", "name"]))
}

fn order(v: &Value) -> Option<Order> {
    let number = s(v, &["orderNumber"])?;
    let store = merchant(v)?;
    let mut items = Vec::new();
    for key in ["orderedItem", "acceptedOffer"] {
        let list = match v.get(key) {
            Some(Value::Array(a)) => a.clone(),
            Some(x) => vec![x.clone()],
            None => continue,
        };
        for it in list {
            let name = s(&it, &["itemOffered", "name"])
                .or_else(|| s(&it, &["orderedItem", "name"]))
                .or_else(|| s(&it, &["name"]));
            let qty = s(&it, &["orderQuantity"]).or_else(|| s(&it, &["eligibleQuantity", "value"]));
            if let Some(n) = name {
                items.push(match qty.filter(|q| q != "1") {
                    Some(q) => format!("{n} ×{q}"),
                    None => n,
                });
            }
        }
    }
    let total = s(v, &["price"])
        .zip(s(v, &["priceCurrency"]))
        .or_else(|| {
            s(v, &["partOfInvoice", "totalPaymentDue", "price"])
                .zip(s(v, &["partOfInvoice", "totalPaymentDue", "priceCurrency"]))
        })
        .map(|(p, c)| format!("{c} {p}"));
    Some(Order {
        key: format!("{}:{number}", store.to_lowercase()),
        store,
        number,
        ordered: s(v, &["orderDate"]),
        items,
        total,
        status: status(v),
        eta: None,
        tracking_url: s(v, &["orderDelivery", "trackingUrl"]),
        return_days: s(v, &["hasMerchantReturnPolicy", "merchantReturnDays"])
            .and_then(|d| d.parse().ok()),
    })
}

fn parcel(v: &Value) -> Option<Order> {
    let o = v.get("partOfOrder")?;
    let number = s(o, &["orderNumber"])?;
    let store = merchant(o).or_else(|| s(v, &["provider", "name"]))?;
    let delivered =
        s(v, &["deliveryStatus"]).is_some_and(|d| d.to_lowercase().contains("delivered"));
    Some(Order {
        key: format!("{}:{number}", store.to_lowercase()),
        store,
        number,
        status: if delivered { 4 } else { status(v).max(2) },
        eta: s(v, &["expectedArrivalUntil"]).or_else(|| s(v, &["expectedArrivalFrom"])),
        tracking_url: s(v, &["trackingUrl"]),
        ..Default::default()
    })
}

// --- Bookings read by the model ----------------------------------------------------------

const MONTHS: [&[&str]; 12] = [
    &["jan"],
    &["feb"],
    &["mar"],
    &["apr"],
    &["mai", "maj", "may"],
    &["jun"],
    &["jul"],
    &["aug"],
    &["sep"],
    &["okt", "oct"],
    &["nov"],
    &["des", "dec"],
];

/// Does the text state this date ("2026-09-14", "14.09.2026", "14/9/2026", "14. sep")?
fn states_date(text: &str, iso: &str) -> bool {
    let (Some(y), Some(m), Some(d)) = (
        iso.get(0..4).and_then(|s| s.parse::<u32>().ok()),
        iso.get(5..7).and_then(|s| s.parse::<usize>().ok()),
        iso.get(8..10).and_then(|s| s.parse::<u32>().ok()),
    ) else {
        return false;
    };
    if !(1..=12).contains(&m) {
        return false;
    }
    let t = text.to_lowercase();
    let numeric = [
        format!("{y}-{m:02}-{d:02}"),
        format!("{d:02}.{m:02}.{y}"),
        format!("{d}.{m}.{y}"),
        format!("{d:02}/{m:02}/{y}"),
        format!("{d}/{m}/{y}"),
        format!("{d:02}.{m:02}.{}", y % 100),
    ];
    numeric.iter().any(|n| t.contains(n.as_str()))
        || MONTHS[m - 1]
            .iter()
            .any(|mon| t.contains(&format!("{d}. {mon}")) || t.contains(&format!("{d} {mon}")))
}

/// The mail states this time of day ("16:00", "16.00", "kl. 16").
fn states_time(text: &str, hhmm: &str) -> bool {
    let Some((h, m)) = hhmm.split_once(':') else {
        return false;
    };
    let t = text.to_lowercase();
    let short = h.trim_start_matches('0');
    let mut forms = vec![format!("{h}:{m}"), format!("{h}.{m}")];
    if !short.is_empty() {
        forms.extend([format!(" {short}:{m}"), format!(" {short}.{m}")]);
        if m == "00" {
            forms.extend([format!("kl. {short}"), format!("kl {short}")]);
        }
    }
    forms.iter().any(|f| t.contains(f.as_str()))
}

/// What the model read from a mail; checked against the mail before use.
#[derive(Debug, Default, Clone, Copy)]
pub struct ModelRead<'a> {
    pub kind: &'a str,
    pub title: &'a str,
    /// Booking or order number.
    pub reference: &'a str,
    /// A booking's start, or the order date.
    pub start: &'a str,
    pub end: &'a str,
    pub provider: &'a str,
    pub city: &'a str,
    pub total: &'a str,
    /// The order will be shipped (else it's digital, collected or ongoing).
    pub ships: bool,
}

fn squash(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
}

/// A booking or order number the mail states: it has digits, isn't a
/// sentence and isn't a date.
pub fn stated_number<'a>(text: &str, reference: &'a str) -> Option<&'a str> {
    let r = reference.trim();
    let parts: Vec<&str> = r.split(['/', '.', '-']).filter(|p| !p.is_empty()).collect();
    let looks_like_date =
        parts.len() == 3 && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit()));
    let ok = r.chars().filter(char::is_ascii_alphanumeric).count() >= 4
        && r.chars().any(|c| c.is_ascii_digit())
        && r.split_whitespace().count() <= 2
        && !looks_like_date
        && squash(text).contains(&squash(r));
    ok.then_some(r)
}

/// An ISO date or date-time as unix seconds; civil times are local.
fn local_ts(s: &str) -> Option<i64> {
    let s = s.trim();
    if let Ok(t) = s.parse::<jiff::Timestamp>() {
        return Some(t.as_second());
    }
    let tz = jiff::tz::TimeZone::system();
    let z = match s.parse::<jiff::civil::DateTime>() {
        Ok(dt) => dt.to_zoned(tz).ok()?,
        Err(_) => s
            .get(..10)?
            .parse::<jiff::civil::Date>()
            .ok()?
            .to_zoned(tz)
            .ok()?,
    };
    Some(z.timestamp().as_second())
}

/// A booking the model read, kept only if the email states its reference and
/// start date, and it starts after the mail was sent (a receipt's date is the
/// purchase itself). The model locates facts; it is never their source.
pub fn checked_booking(text: &str, sent: Option<i64>, m: &ModelRead) -> Option<Booking> {
    let reference = stated_number(text, m.reference)?;
    let date = |s: &str| {
        let s = s.trim();
        (s.len() >= 10 && states_date(text, s)).then(|| s.to_string())
    };
    // A time the mail doesn't state (the model's midnight) is dropped.
    let date = |s: &str| {
        date(s).map(|d| match d.get(11..16) {
            Some(t) if states_time(text, t) => d[..16].to_string(),
            _ => d[..10].to_string(),
        })
    };
    let start = date(m.start)?;
    let kind = match m.kind {
        k @ ("hotel" | "flight" | "train" | "bus" | "table" | "event") => k,
        _ => "event",
    };
    if let Some(sent) = sent
        && local_ts(&start)? <= sent
    {
        return None;
    }
    let opt = |s: &str| (!s.trim().is_empty()).then(|| s.trim().to_string());
    Some(Booking {
        kind: kind.into(),
        key: format!("{kind}:{reference}"),
        reference: reference.into(),
        title: opt(m.title)
            .or_else(|| opt(m.provider))
            .unwrap_or_else(|| "Booking".into()),
        start: Some(start),
        end: date(m.end),
        provider: opt(m.provider),
        city: opt(m.city),
        seat: None,
        party: None,
    })
}

/// A purchase, rental or subscription the model read, kept only if the mail
/// states its order number; a total or date the mail doesn't state is dropped.
pub fn checked_order(text: &str, m: &ModelRead) -> Option<Order> {
    let number = stated_number(text, m.reference)?;
    let store = m.provider.trim();
    if store.is_empty() {
        return None;
    }
    let total = m.total.trim();
    let total =
        (!total.is_empty() && squash(text).contains(&squash(total))).then(|| total.to_string());
    let date = m.start.trim();
    let ordered = (date.len() >= 10 && states_date(text, date)).then(|| date[..10].to_string());
    // A receipt states a price or a date; an account confirmation neither.
    if total.is_none() && ordered.is_none() {
        return None;
    }
    Some(Order {
        key: format!("{}:{number}", store.to_lowercase()),
        store: store.into(),
        number: number.into(),
        ordered,
        items: (!m.title.trim().is_empty())
            .then(|| m.title.trim().to_string())
            .into_iter()
            .collect(),
        total,
        // Nothing to ship: done when it's paid.
        status: if m.ships { 1 } else { 4 },
        eta: None,
        tracking_url: None,
        return_days: None,
    })
}

#[cfg(test)]
mod model_booking_tests {
    use super::*;

    const SENT: Option<i64> = Some(1_788_000_000); // 2026-08-29

    #[test]
    fn keeps_only_what_the_mail_says() {
        let text = "Billetten din er bekreftet. Bestillingsnr. 1234567. Från: 2026-10-24 10:00 Till: 2026-10-24 17:00";
        let read = ModelRead {
            kind: "event",
            title: "Daftöland",
            reference: "1234567",
            start: "2026-10-24T10:00",
            end: "2026-10-24T17:00",
            provider: "Daftö",
            city: "Strömstad",
            ..Default::default()
        };
        let b = checked_booking(text, SENT, &read).unwrap();
        assert_eq!(
            (b.key.as_str(), b.start.as_deref(), b.end.as_deref()),
            (
                "event:1234567",
                Some("2026-10-24T10:00"),
                Some("2026-10-24T17:00")
            )
        );
        let with = |reference, start| ModelRead {
            reference,
            start,
            ..read
        };
        assert!(
            checked_booking(text, SENT, &with("7654321", "2026-10-24")).is_none(),
            "an invented number"
        );
        assert!(
            checked_booking(text, SENT, &with("1234567", "2026-11-02")).is_none(),
            "an invented date"
        );
        assert!(
            checked_booking(text, Some(1_793_000_000), &read).is_none(),
            "a booking starts after its confirmation"
        );
        let promo = "Bo hos oss 04/10/2026. Kjøp av Apple TV gjennom Viaplay fra 2026-10-04.";
        assert!(
            checked_booking(promo, SENT, &with("04/10/2026", "2026-10-04")).is_none(),
            "a date is not a number"
        );
        assert!(
            checked_booking(
                promo,
                SENT,
                &with("Kjøp av Apple TV gjennom Viaplay", "2026-10-04")
            )
            .is_none(),
            "a sentence is not a number"
        );
        let sub = "Ordrebekreftelse. Ordrenummer o-4471902. Startdato 2026-09-02.";
        let b = checked_booking(
            sub,
            SENT,
            &ModelRead {
                reference: "o-4471902",
                start: "2026-09-02T00:00:00",
                ..read
            },
        )
        .unwrap();
        assert_eq!(
            b.start.as_deref(),
            Some("2026-09-02"),
            "a time the mail doesn't state is dropped"
        );
        let b = checked_booking(
            "Ankomst 14. sep 2026, bokningsnr AB-99812",
            SENT,
            &ModelRead {
                kind: "hotel",
                reference: "AB-99812",
                start: "2026-09-14",
                end: "2026-09-16",
                provider: "Daftö",
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            (b.title.as_str(), b.end),
            ("Daftö", None),
            "an end the mail doesn't state is dropped"
        );
    }

    #[test]
    fn a_code_is_not_a_receipt() {
        let read = ModelRead {
            reference: "608927",
            provider: "Xiaomi",
            ..Default::default()
        };
        assert!(checked_order("Bekreftelse av Mi-konto. Koden din er 608927.", &read).is_none());
    }

    #[test]
    fn orders() {
        let text = "Takk for kjøpet! Ordrenummer: 700098765432101. Dato: 14.10.2026. Valheim. Totalt: 199,00 kr";
        let read = ModelRead {
            title: "Valheim",
            reference: "700098765432101",
            start: "2026-10-14",
            provider: "PlayStation Store",
            total: "199,00 kr",
            ..Default::default()
        };
        let o = checked_order(text, &read).unwrap();
        assert_eq!(
            (
                o.key.as_str(),
                o.ordered.as_deref(),
                o.total.as_deref(),
                o.status,
                o.items
            ),
            (
                "playstation store:700098765432101",
                Some("2026-10-14"),
                Some("199,00 kr"),
                4,
                vec!["Valheim".to_string()]
            )
        );
        let o = checked_order(
            text,
            &ModelRead {
                total: "299 kr",
                ships: true,
                ..read
            },
        )
        .unwrap();
        assert_eq!(
            (o.total, o.status),
            (None, 1),
            "an unstated total is dropped; shipped starts as ordered"
        );
        assert!(
            checked_order(
                text,
                &ModelRead {
                    reference: "123456",
                    ..read
                }
            )
            .is_none()
        );
    }
}
