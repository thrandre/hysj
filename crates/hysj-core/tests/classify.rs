//! Rules tier against fixture mails.

use hysj_core::classify::{Context, classify, recipients};
use hysj_proto::{Bucket, Decision, Reason};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/{name}.eml",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn ctx() -> Context {
    let mut c = Context::default();
    c.identities.insert("me@example.com".into());
    c.contacts
        .insert("aisha@example.com".into(), (Some("Aisha Rahman".into()), 3));
    c
}

fn sorted(name: &str, ctx: &Context) -> (Bucket, Reason) {
    classify(&fixture(name), ctx)
}

#[test]
fn rules() {
    let c = ctx();
    use Bucket::*;
    let bulk = |s: &str| Reason::Bulk { signal: s.into() };
    assert_eq!(
        sorted("known_person", &c),
        (People, Reason::EmailedBefore { times: 3 }),
        "address case-insensitive"
    );
    assert_eq!(sorted("newsletter", &c), (Digest, bulk("List-Unsubscribe")));
    assert_eq!(
        sorted("mailing_list", &c),
        (Digest, bulk("List-Id")),
        "bulk beats a known sender"
    );
    assert_eq!(sorted("noreply", &c), (Digest, Reason::NoReply));
    assert_eq!(
        sorted("auto_submitted_no", &c),
        (People, Reason::EmailedBefore { times: 3 }),
        "`no` = sent by a human"
    );
    assert_eq!(sorted("auto_reply", &c), (Digest, bulk("Auto-Submitted")));
    assert_eq!(
        sorted("introduced", &c),
        (
            People,
            Reason::Introduced {
                by: "Aisha Rahman".into()
            }
        )
    );
    assert_eq!(
        sorted("only_to_me", &c),
        (Screener, Reason::FirstTime),
        "you and the sender never introduce"
    );
    assert_eq!(sorted("first_time", &c), (Screener, Reason::FirstTime));
    assert_eq!(
        sorted("receipt", &c),
        (Digest, Reason::Receipt),
        "a stranger's order confirmation skips the Screener"
    );
    assert_eq!(
        sorted("noreply_no", &c),
        (Digest, Reason::NoReply),
        "Norwegian no-reply"
    );
    let service = |s: &str| Reason::DeliveryService { service: s.into() };
    assert_eq!(
        sorted("delivery_service", &c),
        (Digest, service("Amazon SES")),
        "specific service beats Feedback-ID"
    );
    assert_eq!(
        sorted("helpdesk_known", &c),
        (People, Reason::EmailedBefore { times: 3 }),
        "someone you know stays in People"
    );
}

#[test]
fn decisions_win() {
    let mut c = ctx();
    c.decisions
        .insert("news@weeklybyte.example".into(), Decision::Allow);
    c.decisions
        .insert("aisha@example.com".into(), Decision::Block);
    c.decisions
        .insert("stranger@else.example".into(), Decision::Digest);
    let d = |decision| Reason::Decision { decision };
    assert_eq!(
        sorted("newsletter", &c),
        (Bucket::People, d(Decision::Allow))
    );
    assert_eq!(
        sorted("known_person", &c),
        (Bucket::Blocked, d(Decision::Block))
    );
    assert_eq!(
        sorted("first_time", &c),
        (Bucket::Digest, d(Decision::Digest))
    );
}

#[test]
fn all_recipients() {
    let r: Vec<_> = recipients(&fixture("introduced"))
        .into_iter()
        .map(|(a, _)| a)
        .collect();
    assert_eq!(
        r,
        [
            "me@example.com",
            "aisha@example.com",
            "someone@else.example"
        ]
    );
}

#[test]
fn desk_intent_and_replies() {
    use hysj_core::classify::{answered, intent};
    use hysj_proto::Intent;
    let mut c = ctx();
    assert_eq!(
        intent(&fixture("known_person"), &c),
        (Intent::Reply, false),
        "To you"
    );
    assert_eq!(intent(&fixture("introduced"), &c), (Intent::Reply, false));
    let cc_only =
        b"From: a@x.example\r\nTo: team@x.example\r\nCc: Me@Example.com\r\nMessage-ID: <cc@x>\r\n";
    assert_eq!(intent(cc_only, &c), (Intent::Know, false), "only Cc'd");
    let list = b"From: a@x.example\r\nTo: team@x.example\r\nMessage-ID: <l@x>\r\n";
    assert_eq!(
        intent(list, &c),
        (Intent::Read, false),
        "not addressed to you"
    );

    // A sent reply referencing the message marks it answered.
    let reply = b"From: me@example.com\r\nTo: aisha@example.com\r\nIn-Reply-To: <k1@example.com>\r\nReferences: <k0@example.com> <k1@example.com>\r\n";
    assert_eq!(
        answered(reply),
        ["k1@example.com", "k0@example.com", "k1@example.com"]
    );
    c.replied.extend(answered(reply));
    assert_eq!(intent(&fixture("known_person"), &c), (Intent::Know, true));
}
