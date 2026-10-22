//! The Digest as an edition: the mail of one delivery sorted into stories and
//! written up like a short newspaper (Digest mockup). Facts in the words —
//! counts, names, dates — come from the mail through templates, never from a
//! model. Pure: the daemon passes in the delivery's messages.
//!
//! ponytail: stories are sorted by sender and subject words. Per-source
//! extractors (FINN listings, Spond activities) fill in items next; de-duping
//! across alerts and "New / Price dropped" across editions after that.

use std::collections::HashSet;

use hysj_proto::{Control, Edition, Ending, Item, MessageSummary, Reason, Section, Story};

use crate::store::StoredListing;
use jiff::Zoned;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Listing,
    Activity,
    Reading,
    Social,
    Service,
    Other,
    Promotion,
}

fn domain(addr: &str) -> String {
    addr.rsplit_once('@')
        .map(|(_, d)| d.to_ascii_lowercase())
        .unwrap_or_default()
}

fn ends_with_domain(addr: &str, d: &str) -> bool {
    let dom = domain(addr);
    dom == d || dom.ends_with(&format!(".{d}"))
}

const SOCIAL: &[&str] = &[
    "snapchat.com",
    "pinterest.com",
    "facebookmail.com",
    "instagram.com",
    "x.com",
    "twitter.com",
    "tiktok.com",
    "strava.com",
    "discord.com",
    "reddit.com",
];

/// Sale and offer words (Norwegian and English) that mark a promotion.
pub fn promotional(subject: &str) -> bool {
    let s = subject.to_lowercase();
    [
        "%",
        "rabatt",
        "salg",
        "tilbud",
        "kampanje",
        "kupp",
        "spar ",
        "fri frakt",
        "gratis frakt",
        "priser",
        "black friday",
        "sale",
        "offer",
        "discount",
        "deal",
        "wow",
        "kun i dag",
        "only today",
        "nyheter til",
        "kjøp ",
        "shop ",
        "handle ",
    ]
    .iter()
    .any(|w| s.contains(w))
}

/// Mail that wants something from you: an answer, a payment, an action.
/// Mail that wants something from the reader: a failed payment, an action
/// asked for, a deadline, a message waiting. Not confirmations, receipts or
/// invitations to events (those are mostly marketing).
fn needs_you(subject: &str) -> bool {
    if crate::classify::signal(subject, b"") == Some("payment") {
        return true;
    }
    let s = subject.to_lowercase();
    [
        "kunne ikke fornyes",
        "payment failed",
        "action required",
        "action advised",
        "handling kreves",
        "svar innen",
        "reply by",
        "new message",
        "ny melding",
        "nye meldinger",
        "sendte deg",
    ]
    .iter()
    .any(|w| s.contains(w))
}

fn kind(m: &MessageSummary, shops: &HashSet<String>, offers: &HashSet<String>) -> Kind {
    if offers.contains(&m.id) {
        return Kind::Promotion;
    }
    let a = m.from_addr.to_ascii_lowercase();
    if crate::extract::source(&a) {
        return Kind::Listing;
    }
    if ends_with_domain(&a, "spond.com") {
        return Kind::Activity;
    }
    if ends_with_domain(&a, "linkedin.com") {
        return if a.starts_with("newsletters") {
            Kind::Reading
        } else {
            Kind::Social
        };
    }
    if SOCIAL.iter().any(|d| ends_with_domain(&a, d)) {
        return Kind::Social;
    }
    match &m.reason {
        Reason::Bulk { signal } if signal != "Auto-Submitted" => {
            if promotional(&m.subject) || shops.contains(&a) {
                Kind::Promotion
            } else {
                Kind::Reading
            }
        }
        Reason::Bulk { .. } | Reason::NoReply | Reason::DeliveryService { .. } => Kind::Service,
        _ => Kind::Other,
    }
}

// --- Words -----------------------------------------------------------------------

const WORDS: [&str; 11] = [
    "no", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
];

/// "three" up to ten, then digits.
fn word(n: usize) -> String {
    WORDS
        .get(n)
        .map_or_else(|| n.to_string(), |w| (*w).to_string())
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{} {}", word(n), if n == 1 { one } else { many })
}

/// "A, B and C".
fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [a] => a.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn date(ts: Option<i64>) -> String {
    ts.and_then(|t| jiff::Timestamp::from_second(t).ok())
        .map(|t| {
            t.to_zoned(jiff::tz::TimeZone::system())
                .strftime("%-d %b")
                .to_string()
        })
        .unwrap_or_default()
}

fn item(m: &MessageSummary) -> Item {
    Item {
        id: m.id.clone(),
        title: subject(&m.subject),
        sub: format!("{} · {}", m.from, date(m.ts)),
        meta: String::new(),
        text: None,
        tag: needs_you(&m.subject).then(|| "Needs you".to_string()),
        link: None,
        image: None,
    }
}

fn subject(s: &str) -> String {
    if s.trim().is_empty() {
        "(no subject)".into()
    } else {
        s.trim().to_string()
    }
}

/// One line per sender ("Sender — 4 · latest subject"), biggest first.
fn by_sender(msgs: &[&MessageSummary]) -> Vec<Item> {
    let mut groups: Vec<Vec<&MessageSummary>> = Vec::new();
    for &m in msgs {
        match groups
            .iter_mut()
            .find(|g| g[0].from_addr.eq_ignore_ascii_case(&m.from_addr))
        {
            Some(g) => g.push(m),
            None => groups.push(vec![m]),
        }
    }
    groups.sort_by_key(|g| std::cmp::Reverse(g.len()));
    groups
        .into_iter()
        .map(|g| {
            let m = g[0];
            Item {
                id: m.id.clone(),
                title: m.from.clone(),
                sub: subject(&m.subject),
                meta: if g.len() > 1 {
                    format!("{} emails", g.len())
                } else {
                    date(m.ts)
                },
                text: None,
                tag: g
                    .iter()
                    .any(|m| needs_you(&m.subject))
                    .then(|| "Needs you".to_string()),
                link: None,
                image: None,
            }
        })
        .collect()
}

fn section(layout: &str, items: Vec<Item>, shown: usize) -> Section {
    Section {
        title: None,
        layout: layout.into(),
        shown: shown.min(items.len()) as u32,
        items,
    }
}

/// FINN's search name from an alert subject ("Nye annonser: Østfold, IT").
fn finn_search(s: &str) -> String {
    let s = s.trim();
    for p in ["Nye annonser:", "Nye treff:", "New listings:"] {
        if let Some(rest) = s.strip_prefix(p) {
            return rest.trim().to_string();
        }
    }
    s.to_string()
}

// --- The edition -------------------------------------------------------------------

/// What the builder needs besides the messages.
pub struct Context<'a> {
    pub pick: u8,
    /// The delivery and the one before it (`None` for "older").
    pub slot: &'a Zoned,
    pub previous: Option<&'a Zoned>,
    pub next: &'a Zoned,
    pub now: &'a Zoned,
    /// Bookings and orders filed from mail (shown on the latest delivery).
    pub filed: usize,
    /// Listings read from the delivery's alert mail.
    pub listings: &'a [StoredListing],
    /// Senders whose mail is mostly sales (see [`shops`]): all their mail is
    /// filed as promotions.
    pub shops: &'a HashSet<String>,
    /// The reader lets FINN's photos load (else listings show placeholders).
    pub photos: bool,
    /// FINN kinds the reader paused ("cabin"), and "only price drops".
    pub finn_paused: &'a [String],
    pub finn_drops_only: bool,
    /// Newsletter issues the model found to be offers (message ids).
    pub offers: &'a HashSet<String>,
}

/// Senders (lowercase) whose bulk mail is at least a quarter sale offers, over
/// what they've sent: shops, even when this week's subject has no sale word.
pub fn shops(history: &[MessageSummary]) -> HashSet<String> {
    let mut seen: std::collections::HashMap<String, (usize, usize)> = Default::default();
    for m in history {
        if matches!(&m.reason, Reason::Bulk { signal } if signal != "Auto-Submitted") {
            let e = seen.entry(m.from_addr.to_ascii_lowercase()).or_default();
            e.0 += 1;
            e.1 += usize::from(promotional(&m.subject));
        }
    }
    seen.into_iter()
        .filter(|(_, (n, promo))| *n >= 2 && promo * 4 >= *n)
        .map(|(a, _)| a)
        .collect()
}

/// "4 250 000 kr" → 4250000.
fn amount(price: &str) -> Option<u64> {
    let digits: String = price.chars().filter(char::is_ascii_digit).collect();
    digits.parse().ok()
}

fn dropped(l: &StoredListing) -> bool {
    matches!((l.prev_price.as_deref().and_then(amount), amount(&l.price)), (Some(a), Some(b)) if b < a)
}

const KINDS: [(&str, &str, &str, &str, usize); 5] = [
    // kind, one, many, layout, shown
    ("home", "home", "homes", "cards", 3),
    ("job", "job", "jobs", "list", 4),
    ("cabin", "cabin", "cabins", "compact", 3),
    ("plot", "plot", "plots", "list", 3),
    ("item", "item for sale", "items for sale", "list", 3),
];

/// What the reader calls each kind's alerts ("Pause cabin alerts").
fn alert_name(kind: &str) -> &'static str {
    match kind {
        "home" => "home",
        "job" => "job",
        "cabin" => "cabin",
        "plot" => "plot",
        _ => "marketplace",
    }
}

/// FINN's titles run on: "Begby - Moderne 4-roms | Balkong | Heis". The
/// first part is the title, the next two are facts.
fn split_title(t: &str) -> (String, String) {
    let mut parts = t.split(" | ").map(str::trim).filter(|p| !p.is_empty());
    let title = parts.next().unwrap_or(t).to_string();
    (title, parts.take(2).collect::<Vec<_>>().join(" · "))
}

/// FINN's story from its listings: sections by kind, facts in the heading,
/// what's notable in the paragraph, and the reader's settings applied.
fn finn_story(
    all: &[StoredListing],
    ctx: &Context,
) -> (String, String, Vec<Section>, Vec<Control>) {
    let since = ctx.previous.map(|p| p.timestamp().as_second());
    let changed = |l: &StoredListing| dropped(l) || l.title.to_lowercase().contains("ny pris");
    let listings: Vec<&StoredListing> = all
        .iter()
        .filter(|l| !ctx.finn_paused.contains(&l.kind))
        .filter(|l| !ctx.finn_drops_only || changed(l))
        .collect();
    let mut parts = Vec::new();
    let mut sections = Vec::new();
    let mut biggest: Option<(&str, usize)> = None;
    for (kind, one, many, layout, shown) in KINDS {
        let of: Vec<&&StoredListing> = listings.iter().filter(|l| l.kind == kind).collect();
        if of.is_empty() {
            continue;
        }
        if biggest.is_none_or(|(_, n)| of.len() > n) {
            biggest = Some((kind, of.len()));
        }
        parts.push(format!(
            "{} {}",
            of.len(),
            if of.len() == 1 { one } else { many }
        ));
        let items = of
            .iter()
            .map(|l| {
                let (title, facts) = split_title(&l.title);
                // Jobs: "Employer, Role" in the detail; the title says the role.
                let sub = if kind == "job" {
                    vec![
                        l.detail.split(',').next().unwrap_or_default().trim(),
                        l.place.as_str(),
                    ]
                } else if !facts.is_empty() {
                    vec![l.place.as_str(), facts.as_str()]
                } else {
                    vec![l.place.as_str(), l.detail.as_str()]
                };
                Item {
                    id: l.message.clone(),
                    title,
                    sub: sub
                        .into_iter()
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join(" · "),
                    meta: l.price.clone(),
                    text: None,
                    tag: if dropped(l) {
                        Some("Price dropped".to_string())
                    } else if l.title.to_lowercase().contains("ny pris") {
                        Some("New price".to_string())
                    } else if since.is_some_and(|s| l.first_seen > s) {
                        Some("New".to_string())
                    } else {
                        None
                    },
                    link: Some(l.link.clone()),
                    image: if ctx.photos {
                        l.image.as_deref().map(crate::finn::https)
                    } else {
                        None
                    },
                }
            })
            .collect();
        sections.push(Section {
            title: Some(cap(many)),
            ..section(layout, items, shown)
        });
    }
    let head = if parts.is_empty() {
        "FINN sent alerts, but none match your settings.".to_string()
    } else {
        format!("FINN found you {}.", and_list(&parts))
    };

    // What's notable, from the listings themselves.
    let mut para: Vec<String> = listings
        .iter()
        .filter(|l| dropped(l))
        .take(2)
        .map(|l| {
            let at = if l.place.is_empty() {
                String::new()
            } else {
                format!(" in {}", l.place)
            };
            format!(
                "The {}{at} has dropped in price, from {} to {}.",
                KINDS
                    .iter()
                    .find(|k| k.0 == l.kind)
                    .map_or("listing", |k| k.1),
                l.prev_price.clone().unwrap_or_default(),
                l.price
            )
        })
        .collect();
    if para.is_empty() {
        let mut homes: Vec<(&&StoredListing, u64)> = listings
            .iter()
            .filter(|l| l.kind == "home")
            .filter_map(|l| amount(&l.price).map(|a| (l, a)))
            .collect();
        homes.sort_by_key(|(_, a)| *a);
        if homes.len() >= 2 {
            let (l, _) = homes[0];
            let at = if l.place.is_empty() {
                String::new()
            } else {
                format!(", in {}", l.place)
            };
            para.push(format!("The cheapest home is {}{at}.", l.price));
        }
    }
    if let Some(s) = since {
        let seen = listings.iter().filter(|l| l.first_seen <= s).count();
        let at = ctx
            .previous
            .map(|p| p.strftime("%H:%M").to_string())
            .unwrap_or_default();
        match seen {
            0 if para.is_empty() => para.push(match listings.len() {
                1 => format!("It’s new since {at}."),
                2 => format!("Both are new since {at}."),
                n => format!("All {} are new since {at}.", word(n)),
            }),
            0 => {}
            n if n == listings.len() => {
                para.push("You’ve seen all of them in earlier digests.".into())
            }
            1 => para.push("One of them was in an earlier digest.".into()),
            n => para.push(format!(
                "{} of them were in earlier digests.",
                cap(&word(n))
            )),
        }
    }
    let paused: Vec<String> = ctx
        .finn_paused
        .iter()
        .filter(|k| all.iter().any(|l| &l.kind == *k))
        .map(|k| format!("{} alerts", alert_name(k)))
        .collect();
    if !paused.is_empty() {
        para.push(format!("{} are paused.", cap(&and_list(&paused))));
    }
    if ctx.finn_drops_only {
        para.push("Only price changes are shown.".into());
    }

    // "Getting too many?": the reader's levers, each a setting.
    let mut controls = Vec::new();
    if let Some((kind, n)) = biggest
        && n >= 3
    {
        controls.push(Control {
            label: format!("Pause {} alerts", alert_name(kind)),
            key: format!("finn_pause:{kind}"),
            on: true,
        });
    }
    controls.push(if ctx.finn_drops_only {
        Control {
            label: "Show every listing again".into(),
            key: "finn_drops_only".into(),
            on: false,
        }
    } else {
        Control {
            label: "Only tell me about price drops".into(),
            key: "finn_drops_only".into(),
            on: true,
        }
    });
    for k in ctx.finn_paused {
        if all.iter().any(|l| &l.kind == k) {
            controls.push(Control {
                label: format!("Resume {} alerts", alert_name(k)),
                key: format!("finn_pause:{k}"),
                on: false,
            });
        }
    }
    (head, para.join(" "), sections, controls)
}

/// A model headline reads like the rest of Hysj: one plain sentence.
pub fn model_headline_ok(h: &str) -> bool {
    let h = h.trim();
    let words: Vec<&str> = h.split_whitespace().collect();
    let title_case = words.len() >= 4
        && words
            .iter()
            .filter(|w| w.chars().next().is_some_and(char::is_uppercase))
            .count()
            * 2
            > words.len();
    h.ends_with('.') && !h.contains(':') && !h.contains('!') && !title_case
}

pub fn build(msgs: &[MessageSummary], ctx: &Context) -> Edition {
    let latest = ctx.pick == 0;
    let of = |k: Kind| -> Vec<&MessageSummary> {
        msgs.iter()
            .filter(|m| kind(m, ctx.shops, ctx.offers) == k)
            .collect()
    };
    let (listings, activities, reading, social, services, other, promos) = (
        of(Kind::Listing),
        of(Kind::Activity),
        of(Kind::Reading),
        of(Kind::Social),
        of(Kind::Service),
        of(Kind::Other),
        of(Kind::Promotion),
    );
    // Each thing once: a service repeating the same alert is one thing.
    let wants = |v: &[&MessageSummary]| {
        v.iter()
            .filter(|m| needs_you(&m.subject))
            .map(|m| (m.from_addr.to_lowercase(), m.subject.trim().to_lowercase()))
            .collect::<HashSet<_>>()
            .len()
    };
    let mut stories: Vec<(usize, Story)> = Vec::new();
    let mut controls = Vec::new();

    let ids: Vec<&str> = listings.iter().map(|m| m.id.as_str()).collect();
    let read: Vec<StoredListing> = ctx
        .listings
        .iter()
        .filter(|l| ids.contains(&l.message.as_str()))
        .cloned()
        .collect();
    if !read.is_empty() {
        let (head, para, sections, finn_controls) = finn_story(&read, ctx);
        controls = finn_controls;
        stories.push((
            0,
            Story {
                kind: "listings".into(),
                label: "Listings".into(),
                head,
                para,
                sections,
            },
        ));
    } else if !listings.is_empty() {
        let mut searches: Vec<String> = listings.iter().map(|m| finn_search(&m.subject)).collect();
        searches.dedup();
        let head = match searches.as_slice() {
            [one] => format!("FINN has new listings for “{one}”."),
            _ => format!(
                "FINN has new listings for {} of your searches.",
                word(searches.len())
            ),
        };
        let items = listings
            .iter()
            .map(|m| Item {
                title: finn_search(&m.subject),
                sub: format!("FINN · {}", date(m.ts)),
                ..item(m)
            })
            .collect();
        stories.push((
            wants(&listings),
            Story {
                kind: "listings".into(),
                label: "Listings".into(),
                head,
                para: String::new(),
                sections: vec![section("list", items, 6)],
            },
        ));
    }
    if !activities.is_empty() {
        let n = activities.len();
        // One line per activity: Spond writes again for each change or reminder.
        let mut items: Vec<Item> = Vec::new();
        for m in &activities {
            if !items.iter().any(|i| i.title == subject(&m.subject)) {
                items.push(Item {
                    sub: format!("Spond · {}", date(m.ts)),
                    ..item(m)
                });
            }
        }
        stories.push((
            wants(&activities),
            Story {
                kind: "activities".into(),
                label: "Activities".into(),
                head: format!("{} from Spond.", cap(&plural(n, "update", "updates"))),
                para: String::new(),
                sections: vec![section("list", items, 6)],
            },
        ));
    }
    if !reading.is_empty() {
        let n = reading.len();
        let items = reading
            .iter()
            .map(|m| Item {
                text: m.gist.clone(),
                ..item(m)
            })
            .collect();
        stories.push((
            0,
            Story {
                kind: "reading".into(),
                label: "Reading".into(),
                head: format!(
                    "{} worth a look.",
                    cap(&plural(n, "newsletter", "newsletters"))
                ),
                para: String::new(),
                sections: vec![section("reading", items, 5)],
            },
        ));
    }
    if !social.is_empty() {
        let lines = by_sender(&social);
        let names: Vec<String> = lines.iter().map(|i| i.title.clone()).take(3).collect();
        let more = lines.len().saturating_sub(3);
        let from = if more > 0 {
            format!("{} and {}", names.join(", "), plural(more, "more", "more"))
        } else {
            and_list(&names)
        };
        let k = wants(&social);
        let head = match k {
            0 => format!(
                "{} from {from}.",
                cap(&plural(social.len(), "notification", "notifications"))
            ),
            1 => format!(
                "A message for you, and {} from {from}.",
                plural(social.len() - 1, "notification", "notifications")
            ),
            k => format!(
                "{} messages for you, among {} from {from}.",
                cap(&word(k)),
                plural(social.len(), "notification", "notifications")
            ),
        };
        stories.push((
            k,
            Story {
                kind: "social".into(),
                label: "Social".into(),
                head,
                para: String::new(),
                sections: vec![section("lines", lines, 6)],
            },
        ));
    }
    if !services.is_empty() {
        let n = services.len();
        let k = wants(&services);
        let head = match (n, k) {
            (1, 0) => "One notification, and it needs nothing from you.".to_string(),
            (_, 0) => format!(
                "{}, and none of them needs you.",
                cap(&plural(n, "notification", "notifications"))
            ),
            (1, _) => "One notification, and it wants something from you.".to_string(),
            (_, 1) => format!(
                "{}, and only one wants something from you.",
                cap(&plural(n, "notification", "notifications"))
            ),
            (_, k) => format!(
                "{}, and {} want something from you.",
                cap(&plural(n, "notification", "notifications")),
                word(k)
            ),
        };
        stories.push((
            k,
            Story {
                kind: "services".into(),
                label: "Accounts & services".into(),
                head,
                para: String::new(),
                sections: vec![section("lines", by_sender(&services), 6)],
            },
        ));
    }
    if latest && ctx.filed > 0 {
        stories.push((
            0,
            Story {
                kind: "money".into(),
                label: "Money".into(),
                head: format!("{}, all filed.", cap(&plural(ctx.filed, "receipt or booking", "receipts and bookings"))),
                para: "Bookings live in Bookings and orders in Orders — shipping updates move them along without reaching you.".into(),
                sections: Vec::new(),
            },
        ));
    }
    if !other.is_empty() {
        let lines = by_sender(&other);
        stories.push((
            wants(&other),
            Story {
                kind: "other".into(),
                label: "Everything else".into(),
                head: format!(
                    "{} from {}.",
                    cap(&plural(other.len(), "more email", "more emails")),
                    plural(lines.len(), "sender", "senders")
                ),
                para: String::new(),
                sections: vec![section("lines", lines, 6)],
            },
        ));
    }
    // FINN first (the reader's own searches, as designed), then what needs
    // you; otherwise the order above.
    stories.sort_by_key(|(k, s)| std::cmp::Reverse((s.kind == "listings", *k > 0)));
    let needs: usize = stories.iter().map(|(k, _)| k).sum();
    let stories: Vec<Story> = stories.into_iter().map(|(_, s)| s).collect();

    let since = ctx.previous.map(|p| p.strftime("%H:%M").to_string());
    let total = msgs.len();
    let headline = if total == 0 {
        match &since {
            Some(t) => format!("Nothing new since {t}."),
            None => "Nothing older left to read.".into(),
        }
    } else {
        let first = match needs {
            0 => "Nothing here needs you.".to_string(),
            1 => "One thing wants your attention.".to_string(),
            k => format!("{} things want your attention.", cap(&word(k))),
        };
        let drop = read.iter().find(|l| dropped(l));
        let second = if let Some(l) = drop {
            Some(format!(
                "A {} on FINN dropped in price.",
                KINDS
                    .iter()
                    .find(|k| k.0 == l.kind)
                    .map_or("listing", |k| k.1)
            ))
        } else if let Some(s) = stories
            .iter()
            .find(|s| s.kind == "listings" && s.head.starts_with("FINN found"))
        {
            Some(s.head.clone())
        } else if !listings.is_empty() {
            Some("FINN has new listings for you.".to_string())
        } else if !reading.is_empty() {
            Some(format!(
                "{} worth reading.",
                cap(&plural(reading.len(), "newsletter", "newsletters"))
            ))
        } else {
            None
        };
        match second {
            Some(s) => format!("{first} {s}"),
            None => first,
        }
    };
    let lede = if total == 0 {
        String::new()
    } else {
        let came = match &since {
            Some(t) if latest => format!(
                "{} came in since {t}.",
                cap(&plural(total, "email", "emails"))
            ),
            _ => format!("{} in this digest.", cap(&plural(total, "email", "emails"))),
        };
        let mut parts = Vec::new();
        if !reading.is_empty() {
            parts.push(format!(
                "{} worth reading",
                plural(reading.len(), "newsletter is", "newsletters are")
            ));
        }
        if !activities.is_empty() {
            parts.push(format!(
                "Spond sent {}",
                plural(activities.len(), "update", "updates")
            ));
        }
        if !promos.is_empty() {
            parts.push(format!(
                "{} went straight to the archive",
                plural(promos.len(), "promotion", "promotions")
            ));
        }
        if parts.is_empty() {
            came
        } else {
            format!("{came} {}.", cap(&and_list(&parts)))
        }
    };
    let minutes = (total - promos.len()).div_ceil(10).max(1);
    let meta = match (ctx.pick, &since) {
        (3, _) | (_, None) => format!("Older · before {}", ctx.slot.strftime("%A %-d %B")),
        (_, Some(t)) => format!(
            "{} · {} digest · since {t} · {minutes} min",
            ctx.slot.strftime("%A"),
            ctx.slot.strftime("%H:%M")
        ),
    };
    let text = if latest {
        let when = if ctx.next.date() == ctx.now.date() {
            "today".to_string()
        } else {
            "tomorrow".to_string()
        };
        format!(
            "That’s everything since {}. Next digest {when} at {}.",
            since.clone().unwrap_or_default(),
            ctx.next.strftime("%H:%M")
        )
    } else {
        "That’s everything in this digest.".into()
    };
    let shops = by_sender(&promos).len();
    let note = match promos.len() {
        0 => String::new(),
        1 => "One promotion went straight to the archive.".into(),
        n => format!(
            "{} from {} went straight to the archive.",
            cap(&plural(n, "promotion", "promotions")),
            plural(shops, "shop", "shops")
        ),
    };
    Edition {
        pick: ctx.pick,
        meta,
        headline,
        lede,
        stories,
        ending: Ending {
            text,
            note,
            promotions: by_sender(&promos),
        },
        ids: msgs.iter().map(|m| m.id.clone()).collect(),
        // Needs the headers: filled in by the daemon (see `classify::official`).
        unsure: Vec::new(),
        controls,
    }
}

/// What the model may write the headline from: the edition's own sentences
/// and item titles. Its text is kept only if it passes [`model_text_ok`].
pub fn facts(e: &Edition) -> String {
    let mut s = format!("Headline: {}\nLede: {}\n", e.headline, e.lede);
    for st in &e.stories {
        s.push_str(&format!("\n{}: {}\n", st.label, st.head));
        if !st.para.is_empty() {
            s.push_str(&format!("{}\n", st.para));
        }
        for it in st.sections.iter().flat_map(|sec| sec.items.iter().take(4)) {
            s.push_str(&format!("- {}\n", it.title));
        }
    }
    if !e.ending.note.is_empty() {
        s.push_str(&format!("\n{}\n", e.ending.note));
    }
    s
}

/// The cache key for model text written from these facts: new facts, new text.
/// ponytail: std's hasher is stable within a toolchain; a new one only rewrites once.
pub fn facts_key(facts: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    facts.hash(&mut h);
    format!("digest:{:016x}", h.finish())
}

/// Number words, from one; words past the list must appear as written.
const NUMBER_WORDS: &[&str] = &[
    "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven",
    "twelve",
];
const OTHER_NUMBER_WORDS: &[&str] = &[
    "dozen", "hundred", "thousand", "million", "half", "single", "couple", "several",
];

/// The numbers in a text, by value ("08:00" and "8", "2" and "two" agree).
fn numbers(s: &str) -> HashSet<String> {
    let lower = s.to_lowercase();
    let digits = lower
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|d| d.parse::<u64>().ok())
        .map(|n| n.to_string());
    let words = lower.split(|c: char| !c.is_alphabetic()).filter_map(|w| {
        match NUMBER_WORDS.iter().position(|n| *n == w) {
            Some(i) => Some((i + 1).to_string()),
            None => OTHER_NUMBER_WORDS.contains(&w).then(|| w.to_string()),
        }
    });
    digits.chain(words).collect()
}

/// The model only smooths the words: every number it writes, in digits or in
/// words, must already be in the facts. Otherwise the templates stand.
pub fn model_text_ok(facts: &str, text: &str) -> bool {
    let len = text.split_whitespace().count();
    (3..=90).contains(&len) && numbers(text).is_subset(&numbers(facts))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hysj_proto::{Bucket, Intent};

    fn m(id: &str, from: &str, addr: &str, subject: &str, reason: Reason) -> MessageSummary {
        MessageSummary {
            id: id.into(),
            account: "a".into(),
            from: from.into(),
            from_addr: addr.into(),
            subject: subject.into(),
            date: None,
            ts: Some(1_792_000_000),
            bucket: Bucket::Digest,
            reason,
            intent: Intent::Know,
            replied: false,
            arrived_on: "me@x".into(),
            gist: None,
            replies: vec![],
            to: None,
            tags: vec![],
        }
    }
    fn bulk() -> Reason {
        Reason::Bulk {
            signal: "List-Unsubscribe".into(),
        }
    }

    fn at(s: &str) -> Zoned {
        format!("{s}[Europe/Oslo]").parse().unwrap()
    }

    #[test]
    fn stories_and_words() {
        let msgs = vec![
            m(
                "1",
                "FINN",
                "agent@finn.no",
                "Nye annonser: Østfold, IT - programvare",
                bulk(),
            ),
            m("2", "FINN", "agent@finn.no", "Nye annonser: begby", bulk()),
            m(
                "3",
                "Spond",
                "noreply@spond.com",
                "Fotballtrening torsdag",
                bulk(),
            ),
            m(
                "4",
                "Illustrert Vitenskap",
                "news@ivt.example",
                "Sync without a server",
                bulk(),
            ),
            m(
                "5",
                "Jollyroom",
                "hello@jollyroom.no",
                "WOW-priser på vinterklær",
                bulk(),
            ),
            m(
                "6",
                "Superkul",
                "kunde@superkul.no",
                "40% rabatt på kostymer",
                bulk(),
            ),
            m(
                "7",
                "Podme",
                "support@podme.example",
                "Abonnement kunne ikke fornyes",
                Reason::NoReply,
            ),
            m(
                "8",
                "Google",
                "no-reply@google.example",
                "Security alert",
                Reason::NoReply,
            ),
            m(
                "9",
                "LinkedIn",
                "messages-noreply@linkedin.com",
                "You have 1 new message",
                bulk(),
            ),
            m(
                "10",
                "LinkedIn",
                "newsletters-noreply@linkedin.com",
                "Claude broke Github",
                bulk(),
            ),
        ];
        let (slot, previous, next, now) = (
            at("2026-10-20T16:00:00+02:00"),
            at("2026-10-20T08:00:00+02:00"),
            at("2026-10-21T08:00:00+02:00"),
            at("2026-10-20T17:00:00+02:00"),
        );
        let e = build(
            &msgs,
            &Context {
                pick: 0,
                slot: &slot,
                previous: Some(&previous),
                next: &next,
                now: &now,
                filed: 2,
                listings: &[],
                shops: &HashSet::new(),
                photos: false,
                finn_paused: &[],
                finn_drops_only: false,
                offers: &HashSet::new(),
            },
        );
        assert_eq!(
            e.headline,
            "Two things want your attention. FINN has new listings for you."
        );
        assert_eq!(
            e.lede,
            "Ten emails came in since 08:00. Two newsletters are worth reading, Spond sent one update and two promotions went straight to the archive."
        );
        assert_eq!(e.meta, "Tuesday · 16:00 digest · since 08:00 · 1 min");
        let kinds: Vec<&str> = e.stories.iter().map(|s| s.kind.as_str()).collect();
        assert_eq!(
            kinds,
            [
                "listings",
                "social",
                "services",
                "activities",
                "reading",
                "money"
            ],
            "FINN first, then what needs you"
        );
        assert_eq!(
            e.stories[2].head,
            "Two notifications, and only one wants something from you."
        );
        assert_eq!(
            e.stories[0].head,
            "FINN has new listings for two of your searches."
        );
        assert_eq!(
            e.stories[0].sections[0].items[0].title,
            "Østfold, IT - programvare"
        );
        assert_eq!(
            e.ending.note,
            "Two promotions from two shops went straight to the archive."
        );
        assert_eq!(
            e.ending.text,
            "That’s everything since 08:00. Next digest tomorrow at 08:00."
        );
        assert_eq!(e.ids.len(), 10);
    }

    #[test]
    fn empty_and_older() {
        let s = at("2026-10-20T08:00:00+02:00");
        let e = build(
            &[],
            &Context {
                pick: 3,
                slot: &s,
                previous: None,
                next: &s,
                now: &s,
                filed: 0,
                listings: &[],
                shops: &HashSet::new(),
                photos: false,
                finn_paused: &[],
                finn_drops_only: false,
                offers: &HashSet::new(),
            },
        );
        assert_eq!(e.headline, "Nothing older left to read.");
        assert!(model_text_ok(
            "FINN found you 2 homes. Three things want you.",
            "Three things want you, and FINN found 2 homes."
        ));
        assert!(
            !model_text_ok("FINN found you 2 homes.", "FINN found you 3 homes."),
            "a new number"
        );
        assert!(model_text_ok(
            "41 emails since 08:00. FINN found you 2 homes.",
            "Since 8:00, 41 emails came in, and two homes."
        ));
        assert!(
            !model_text_ok(
                "FINN found you 2 homes.",
                "FINN found you two homes and four jobs."
            ),
            "a number word"
        );
        assert!(e.stories.is_empty() && e.lede.is_empty());
        assert!(
            e.meta.starts_with("Older · before Tuesday 20 October"),
            "{}",
            e.meta
        );
    }

    #[test]
    fn what_needs_you() {
        for yes in [
            "Abonnement kunne ikke fornyes – prøver igjen",
            "$20.00 payment to Example Cloud was unsuccessful again",
            "Betalingsproblem for HBO Max-abonnementet ditt",
            "[Action Advised] Manage your unused OAuth clients",
            "Ny melding på Helsenorge",
            "Kari Nordmann sendte deg nye meldinger",
        ] {
            assert!(needs_you(yes), "{yes}");
        }
        for no in [
            "Ordrebekreftelse fra TV 2 Play",
            "Kontonummeret ditt er bekreftet",
            "Takk for din betaling!",
            "Vi har behandlet betalingen din",
            "🎫 Din bonussjekk utløper om få dager!",
            "🔥 Invitasjon til investormøte",
            "Payment information",
        ] {
            assert!(!needs_you(no), "{no}");
        }
    }

    #[test]
    fn finn_listings_make_the_story() {
        let l = |key: &str, kind: &str, price: &str, prev: Option<&str>| StoredListing {
            key: key.into(),
            message: "1".into(),
            kind: kind.into(),
            title: format!("Ad {key} | Balkong | Heis | Garasje"),
            price: price.into(),
            place: "Ski".into(),
            detail: "Megler AS, Utvikler".into(),
            link: format!("https://www.finn.no/{key}"),
            image: None,
            // The first was in an earlier digest; the others are new.
            first_seen: if key == "1" { 0 } else { 1_800_000_000 },
            prev_price: prev.map(Into::into),
        };
        let listings = [
            l("1", "home", "4 250 000 kr", Some("4 500 000 kr")),
            l("2", "home", "3 000 000 kr", None),
            l("3", "job", "", None),
        ];
        let msgs = vec![m("1", "FINN", "agent@finn.no", "Nye annonser: Ski", bulk())];
        let t = at("2026-10-20T16:00:00+02:00");
        let e = build(
            &msgs,
            &Context {
                pick: 0,
                slot: &t,
                previous: Some(&t),
                next: &t,
                now: &t,
                filed: 0,
                listings: &listings,
                shops: &HashSet::new(),
                photos: false,
                finn_paused: &[],
                finn_drops_only: false,
                offers: &HashSet::new(),
            },
        );
        let story = &e.stories[0];
        assert_eq!(story.head, "FINN found you 2 homes and 1 job.");
        assert_eq!(
            story.para,
            "The home in Ski has dropped in price, from 4 500 000 kr to 4 250 000 kr. One of them was in an earlier digest."
        );
        assert_eq!(story.sections[0].layout, "cards");
        let card = &story.sections[0].items[0];
        assert_eq!(
            (card.title.as_str(), card.sub.as_str(), card.tag.as_deref()),
            ("Ad 1", "Ski · Balkong · Heis", Some("Price dropped")),
            "short title, facts from the rest"
        );
        assert_eq!(story.sections[0].items[1].tag.as_deref(), Some("New"));
        assert_eq!(story.sections[1].title.as_deref(), Some("Jobs"));
        assert_eq!(
            story.sections[1].items[0].sub, "Megler AS · Ski",
            "employer · place"
        );
        assert_eq!(
            e.controls
                .iter()
                .map(|c| c.label.as_str())
                .collect::<Vec<_>>(),
            ["Only tell me about price drops"],
            "no kind is big enough to pause"
        );
        let quiet = build(
            &msgs,
            &Context {
                pick: 0,
                slot: &t,
                previous: Some(&t),
                next: &t,
                now: &t,
                filed: 0,
                listings: &listings,
                shops: &HashSet::new(),
                photos: false,
                finn_paused: &["job".to_string()],
                finn_drops_only: true,
                offers: &HashSet::new(),
            },
        );
        assert_eq!(quiet.stories[0].head, "FINN found you 1 home.");
        assert!(
            quiet.stories[0]
                .para
                .ends_with("Job alerts are paused. Only price changes are shown.")
        );
        assert_eq!(
            quiet
                .controls
                .iter()
                .map(|c| (c.key.as_str(), c.on))
                .collect::<Vec<_>>(),
            [("finn_drops_only", false), ("finn_pause:job", false)]
        );
        assert!(model_headline_ok(
            "Nothing here needs you, and FINN found a cheaper home."
        ));
        assert!(!model_headline_ok(
            "You've been found: One home and one job opportunity await."
        ));
        assert!(!model_headline_ok("Finn's Success Story For You Today."));
        assert_eq!(
            e.headline,
            "Nothing here needs you. A home on FINN dropped in price."
        );
    }

    #[test]
    fn shops_by_history() {
        let msgs = vec![
            m("1", "Shop", "shop@x.example", "Nyheter i butikken", bulk()),
            m("2", "Shop", "shop@x.example", "40% rabatt i helgen", bulk()),
            m("3", "Shop", "shop@x.example", "Vinterkolleksjonen", bulk()),
            m(
                "4",
                "Paper",
                "news@y.example",
                "Ukens viktigste saker",
                bulk(),
            ),
            m("5", "Paper", "news@y.example", "Leder: om høsten", bulk()),
        ];
        let shops = shops(&msgs);
        assert!(shops.contains("shop@x.example") && !shops.contains("news@y.example"));
        let t = at("2026-10-20T16:00:00+02:00");
        let e = build(
            &msgs[..1],
            &Context {
                pick: 0,
                slot: &t,
                previous: Some(&t),
                next: &t,
                now: &t,
                filed: 0,
                listings: &[],
                shops: &shops,
                photos: false,
                finn_paused: &[],
                finn_drops_only: false,
                offers: &HashSet::new(),
            },
        );
        assert!(
            e.stories.is_empty(),
            "a shop's mail without sale words is still a promotion"
        );
        assert_eq!(e.ending.note, "One promotion went straight to the archive.");
    }
}
