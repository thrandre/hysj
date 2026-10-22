//! Rules tier: deterministic sorting into People / Digest / Screener / Blocked.
//! Cheapest signals first; the first rule that matches decides.

use std::collections::{HashMap, HashSet};

use hysj_proto::{Bucket, Decision, Intent, Reason};
use mail_parser::{Message, MessageParser};

/// What the rules know about the user. All addresses lower case.
#[derive(Debug, Default)]
pub struct Context {
    /// The user's own addresses.
    pub identities: HashSet<String>,
    /// People the user has sent mail to: address → (display name, messages sent).
    /// Never contains an identity.
    pub contacts: HashMap<String, (Option<String>, u32)>,
    /// Screener decisions by sender address.
    pub decisions: HashMap<String, Decision>,
    /// Message-IDs the user has replied to (from sent mail's In-Reply-To/References).
    pub replied: HashSet<String>,
}

pub fn classify(headers: &[u8], ctx: &Context) -> (Bucket, Reason) {
    let Some(msg) = MessageParser::default().parse_headers(headers) else {
        return (Bucket::Screener, Reason::FirstTime);
    };
    let from = msg
        .from()
        .and_then(|f| f.first())
        .and_then(|a| a.address())
        .unwrap_or_default()
        .to_lowercase();

    // 1. The user's own decision beats every heuristic.
    if let Some(&decision) = ctx.decisions.get(&from) {
        let bucket = match decision {
            Decision::Allow => Bucket::People,
            Decision::Digest => Bucket::Digest,
            Decision::Block => Bucket::Blocked,
        };
        return (bucket, Reason::Decision { decision });
    }
    // 2. Bulk mail never reaches People, even from someone you know.
    if let Some(signal) = bulk_signal(&msg) {
        return (
            Bucket::Digest,
            Reason::Bulk {
                signal: signal.into(),
            },
        );
    }
    if is_noreply(&from) {
        return (Bucket::Digest, Reason::NoReply);
    }
    // 3. Someone you've written to.
    if let Some((_, times)) = ctx.contacts.get(&from) {
        return (Bucket::People, Reason::EmailedBefore { times: *times });
    }
    // 4. Introduced: a contact (not you, not the sender) is on the message.
    let others = msg.to().into_iter().chain(msg.cc()).flat_map(|a| a.iter());
    for addr in others {
        let Some(address) = addr.address().map(str::to_lowercase) else {
            continue;
        };
        if address == from || ctx.identities.contains(&address) {
            continue;
        }
        if let Some((name, _)) = ctx.contacts.get(&address) {
            let by = name.clone().unwrap_or(address);
            return (Bucket::People, Reason::Introduced { by });
        }
    }
    // 5. A stranger mailing through a delivery service is a business, not a person.
    // Weaker than bulk headers: a helpdesk reply from someone you know stays in People.
    if let Some(service) = delivery_service(&msg) {
        return (
            Bucket::Digest,
            Reason::DeliveryService {
                service: service.into(),
            },
        );
    }
    // 6. A receipt or order confirmation from a stranger is a business: it's
    // filed (Orders/Bookings) and read in the Digest, never screened.
    let subject = msg.subject().unwrap_or_default().to_lowercase();
    if RECEIPT_SUBJECT.iter().any(|w| subject.contains(w)) {
        return (Bucket::Digest, Reason::Receipt);
    }
    // ponytail: address-book contacts would also skip the Screener; no source for them yet.
    (Bucket::Screener, Reason::FirstTime)
}

/// Subjects only shops and services write. Narrow on purpose: a person's
/// "Booking for Friday?" still goes to the Screener.
const RECEIPT_SUBJECT: &[&str] = &[
    "ordrebekreftelse",
    "orderbekräftelse",
    "order confirmation",
    "bestillingsbekreftelse",
    "bookingbekreftelse",
    "booking confirmation",
    "bokningsbekräftelse",
    "kvittering",
    "kvitto",
    "receipt",
    "takk for kjøpet",
    "takk for din bestilling",
    "takk for bestillingen",
    "tack för din beställning",
    "thank you for your order",
    "thanks for your order",
    "your order",
    "din ordre",
    "din bestilling",
    "faktura",
    "invoice",
];

/// Bump when the rules change: stores re-sort all mail on next open.
pub const RULES_VERSION: u32 = 6;

/// Desk column and whether the user already replied.
/// ponytail: To vs Cc stand-in; the AI tier (step 9) decides Reply / Read / Know.
pub fn intent(headers: &[u8], ctx: &Context) -> (Intent, bool) {
    let Some(msg) = MessageParser::default().parse_headers(headers) else {
        return (Intent::Read, false);
    };
    if msg.message_id().is_some_and(|id| ctx.replied.contains(id)) {
        return (Intent::Know, true);
    }
    let mine = |a: Option<&mail_parser::Address>| {
        a.into_iter()
            .flat_map(|a| a.iter())
            .filter_map(|a| a.address())
            .any(|a| ctx.identities.contains(&a.to_lowercase()))
    };
    let intent = if mine(msg.to()) {
        Intent::Reply
    } else if mine(msg.cc()) {
        Intent::Know
    } else {
        Intent::Read
    };
    (intent, false)
}

/// Which of the user's addresses a message was sent to (To before Cc).
pub fn arrived_on(headers: &[u8], ctx: &Context) -> Option<String> {
    let msg = MessageParser::default().parse_headers(headers)?;
    [msg.to(), msg.cc()]
        .into_iter()
        .flatten()
        .flat_map(|a| a.iter())
        .filter_map(|a| a.address().map(str::to_lowercase))
        .find(|a| ctx.identities.contains(a))
}

/// Message-IDs a sent message answers: its In-Reply-To and References.
pub fn answered(headers: &[u8]) -> Vec<String> {
    let Some(msg) = MessageParser::default().parse_headers(headers) else {
        return Vec::new();
    };
    [msg.in_reply_to(), msg.references()]
        .into_iter()
        .filter_map(|h| h.as_text_list())
        .flatten()
        .map(|id| id.to_string())
        .collect()
}

/// Headers email service providers add; most specific first.
fn delivery_service(msg: &Message) -> Option<&'static str> {
    const ESPS: &[(&str, &str)] = &[
        ("X-SES-Outgoing", "Amazon SES"),
        ("X-SG-EID", "SendGrid"),
        ("X-Mailgun-Sid", "Mailgun"),
        ("X-MC-User", "Mailchimp"),
        ("X-Mandrill-User", "Mandrill"),
        ("X-Mailjet-Campaign", "Mailjet"),
        ("X-SIB-Id", "Brevo"),
        ("X-PM-Message-Id", "Postmark"),
        // Required by Gmail of bulk senders; generic.
        ("Feedback-ID", "a bulk mail service"),
        ("X-Feedback-ID", "a bulk mail service"),
    ];
    ESPS.iter()
        .find(|(h, _)| msg.header_raw(*h).is_some())
        .map(|(_, name)| *name)
}

fn bulk_signal(msg: &Message) -> Option<&'static str> {
    let header = |name: &str| msg.header_raw(name).map(|v| v.trim().to_ascii_lowercase());
    if header("List-Unsubscribe").is_some() {
        return Some("List-Unsubscribe");
    }
    if header("List-Id").is_some() {
        return Some("List-Id");
    }
    if header("Precedence").is_some_and(|p| matches!(p.as_str(), "bulk" | "list" | "junk")) {
        return Some("Precedence");
    }
    // `Auto-Submitted: no` is the explicit "a human sent this".
    if header("Auto-Submitted").is_some_and(|v| v != "no") {
        return Some("Auto-Submitted");
    }
    None
}

fn is_noreply(address: &str) -> bool {
    let local = address.split('@').next().unwrap_or_default();
    [
        "noreply",
        "no-reply",
        "donotreply",
        "do-not-reply",
        "mailer-daemon",
        "no_reply",
        "ikke-svar",
        "ikkesvar",
    ]
    .iter()
    .any(|p| local.contains(p))
}

/// Recipients (To/Cc/Bcc) of a message the user sent: `(address, name)`, lower-cased address.
pub fn recipients(headers: &[u8]) -> Vec<(String, Option<String>)> {
    let Some(msg) = MessageParser::default().parse_headers(headers) else {
        return Vec::new();
    };
    msg.to()
        .into_iter()
        .chain(msg.cc())
        .chain(msg.bcc())
        .flat_map(|a| a.iter())
        .filter_map(|a| Some((a.address()?.to_lowercase(), a.name().map(str::to_string))))
        .collect()
}

/// Public services and the school/kindergarten platforms parents get mail
/// from. Their notifications are held like any others, but the Digest asks
/// once whether they should come through.
const GOVERNMENT: &[&str] = &[
    "helsenorge.no",
    "nav.no",
    "skatteetaten.no",
    "altinn.no",
    "lanekassen.no",
    "politiet.no",
    "vegvesen.no",
    "udi.no",
    "norge.no",
    "brreg.no",
    "husbanken.no",
    "domstol.no",
    "kommune.no",
];
const SCHOOL: &[&str] = &["vigilo.no", "mykid.no", "kidplan.com", "itslearning.com"];

/// Why held mail from this sender might matter, if it might; the caller
/// must also check the sender is authenticated (`extract::authenticated`),
/// or a spoofed "tax office" would be put in front of the reader.
pub fn official(address: &str) -> Option<&'static str> {
    let domain = address.rsplit('@').next()?.to_lowercase();
    let under = |list: &[&str]| {
        list.iter()
            .any(|d| domain == *d || domain.ends_with(&format!(".{d}")))
    };
    if under(GOVERNMENT) {
        Some("Held because it looks like a notification — but it’s from a public service")
    } else if under(SCHOOL) {
        Some("Held because it looks like a notification — but it’s from a school or kindergarten")
    } else {
        None
    }
}

/// Held mail that can't wait for the digest, by kind. Narrow on purpose:
/// these reach the Desk, so the phrases are the services' own, and the
/// caller checks the sender is authenticated (a "card declined" mail is the
/// oldest phishing there is).
const PAYMENT: &[&str] = &[
    "was unsuccessful",
    "payment failed",
    "payment declined",
    "was declined",
    "could not be processed",
    "betalingsproblem",
    "betalingen feilet",
    "betalingen mislyktes",
    "ble avvist",
    "kunne ikke belastes",
    "har forfalt",
    "betalingspåminnelse",
    "purring",
];
const SECURITY: &[&str] = &[
    "sikkerhetsvarsel",
    "security alert",
    "new sign-in",
    "nytt innloggingsforsøk",
    "ny pålogging",
    "ny enhet bruker",
    "new device",
    "suspicious",
    "mistenkelig",
];

/// "payment", "security" or "review", for mail that can't wait.
pub fn signal(subject: &str, headers: &[u8]) -> Option<&'static str> {
    let s = subject.to_lowercase();
    if PAYMENT.iter().any(|w| s.contains(w)) {
        return Some("payment");
    }
    if SECURITY.iter().any(|w| s.contains(w)) {
        return Some("security");
    }
    let h = String::from_utf8_lossy(headers).to_lowercase();
    h.contains("x-github-reason: review_requested")
        .then_some("review")
}

#[cfg(test)]
mod official_tests {
    #[test]
    fn official_senders() {
        assert!(super::official("noreply@helsenorge.no").is_some());
        assert!(super::official("post@fredrikstad.kommune.no").is_some());
        assert!(super::official("donotreply@vigilo.no").is_some());
        assert!(super::official("noreply@helsenorge.no.evil.example").is_none());
        assert!(super::official("news@xxl.no").is_none());
    }

    #[test]
    fn signals() {
        use super::signal;
        assert_eq!(
            signal(
                "$20.00 payment to Example Cloud was unsuccessful again",
                b""
            ),
            Some("payment")
        );
        assert_eq!(
            signal("Betalingsproblem for HBO Max-abonnementet ditt", b""),
            Some("payment")
        );
        assert_eq!(
            signal("Sikkerhetsvarsel for me@gmail.com", b""),
            Some("security")
        );
        assert_eq!(
            signal("Fix login bug", b"X-GitHub-Reason: review_requested\r\n"),
            Some("review")
        );
        assert_eq!(
            signal("Prevent future card declines", b""),
            None,
            "marketing"
        );
        assert_eq!(signal("Netflix: Innloggingskoden din", b""), None, "a code");
    }
}
