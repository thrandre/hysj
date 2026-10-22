use anyhow::Result;
use hysj_ai::{DigestText, Gist, MessageText, Replies, Writer};
use hysj_core::{Changes, RawHeaders, Store};
use hysj_proto::{Account, Bucket};
use hysjd::{Daemon, Hub, ai};

/// Echoes subjects for one sender (to be rejected), answers properly otherwise.
struct Fake;

impl Writer for Fake {
    async fn booking(&self, _: &MessageText<'_>, _: &str) -> Result<hysj_ai::BookingGuess> {
        Ok(hysj_ai::BookingGuess::default())
    }
    async fn sender_kind(&self, name: &str, _: &[String]) -> Result<hysj_ai::SenderKind> {
        let kind = if name.contains("Weekly") {
            "publication"
        } else {
            "shop"
        };
        Ok(hysj_ai::SenderKind { kind: kind.into() })
    }
    async fn gist(&self, m: &MessageText<'_>) -> Result<Gist> {
        let gist = if m.from.contains("Echo") {
            m.subject.to_string()
        } else {
            format!("You are asked about: {}.", m.body.trim())
        };
        Ok(Gist {
            gist,
            confidence: 1.0,
        })
    }
    async fn issue(&self, m: &MessageText<'_>) -> Result<hysj_ai::Issue> {
        Ok(hysj_ai::Issue {
            gist: format!("This week: {}, and more.", m.body.trim()),
            offer: false,
        })
    }
    async fn replies(&self, _: &MessageText<'_>) -> Result<Replies> {
        Ok(Replies {
            replies: vec!["Yes, I'll check today.".into(), "Mvh Aisha".into()],
        })
    }
    async fn digest(&self, facts: &str) -> Result<DigestText> {
        // Rewrites the headline it was given.
        let headline = facts
            .lines()
            .next()
            .unwrap_or_default()
            .trim_start_matches("Headline: ");
        Ok(DigestText {
            headline: format!("So, {headline}"),
            summary: "Worth a read when you have time.".into(),
        })
    }
}

fn rfc2822(ts: i64) -> String {
    jiff::Timestamp::from_second(ts)
        .unwrap()
        .strftime("%a, %d %b %Y %H:%M:%S +0000")
        .to_string()
}

#[tokio::test]
async fn writes_gists_and_replies() {
    let mut store = Store::open_in_memory().unwrap();
    store
        .add_account(&Account {
            name: "gmail".into(),
            address: "me@gmail.com".into(),
            host: "imap.gmail.com".into(),
            port: 993,
            username: "me@gmail.com".into(),
            smtp_host: "smtp.gmail.com".into(),
            smtp_port: 465,
            label: None,
        })
        .unwrap();
    // You've written to Aisha and Echo before: their mail goes to People.
    let sent = |id: &str, to: &str| RawHeaders {
        remote_id: id.into(),
        headers: format!("From: me@gmail.com\r\nTo: {to}\r\n").into_bytes(),
        ..Default::default()
    };
    let now = jiff::Timestamp::now().as_second();
    let slot = hysj_core::digest::last_slot(&jiff::Zoned::now())
        .timestamp()
        .as_second();
    let msg = |id: &str, from: &str, subject: &str, extra: &str, date: i64| {
        RawHeaders {
        remote_id: id.into(),
        headers: format!("From: {from}\r\nTo: me@gmail.com\r\nSubject: {subject}\r\nMessage-ID: <{id}@x>\r\nDate: {}\r\n{extra}", rfc2822(date)).into_bytes(),
        ..Default::default()
    }
    };
    let new = vec![
        msg("p1", "Aisha <aisha@x.example>", "Q3", "", now - 600),
        msg(
            "p2",
            "Echo <echo@x.example>",
            "Lunch on Friday",
            "",
            now - 500,
        ),
        msg(
            "n1",
            "Weekly <news@n.example>",
            "Issue 42",
            "List-Unsubscribe: <mailto:u@n.example>\r\n",
            slot - 60,
        ),
        // Arrived for the next edition: written now, ready when it's due.
        msg(
            "n2",
            "Weekly <news@n.example>",
            "Issue 43",
            "List-Unsubscribe: <mailto:u@n.example>\r\n",
            slot + 1,
        ),
    ];
    let changes = Changes {
        new,
        sent: vec![sent("s1", "aisha@x.example, echo@x.example")],
        ..Default::default()
    };
    store.apply("gmail", &changes).unwrap();
    for (subject, body) in [
        ("Q3", "the Q3 numbers"),
        ("Lunch on Friday", "lunch"),
        ("Issue 42", "this week in tech"),
        ("Issue 43", "next week in tech"),
    ] {
        let id = [Bucket::People, Bucket::Digest]
            .into_iter()
            .flat_map(|b| store.list(b, None).unwrap())
            .find(|m| m.subject == subject)
            .unwrap()
            .id;
        store.set_body(&id, body).unwrap(); // no IMAP in tests
    }
    let hub = Hub::new(Daemon::new(store));

    let model = "fake";
    // People first: both are Reply cards (gist + replies) and fill the first
    // batch; the newsletter and the retry of the rejected gist come next.
    let first = ai::pass(&hub, &Fake, model).await;
    // Near a delivery time, the coming edition's headline is written too.
    assert!(
        (6..=7).contains(&first),
        "a batch of four jobs, the edition's headline and sorting the newsletter's sender: {first}"
    );
    let mut passes = 1;
    while ai::pass(&hub, &Fake, model).await > 0 {
        passes += 1;
        assert!(passes < 5, "the queue drains");
    }

    let d = hub.daemon.lock().await;
    let people = d.store().list(Bucket::People, None).unwrap();
    let aisha = people.iter().find(|m| m.subject == "Q3").unwrap();
    assert_eq!(
        aisha.gist.as_deref(),
        Some("You are asked about: the Q3 numbers.")
    );
    assert_eq!(
        aisha.replies,
        ["Yes, I'll check today."],
        "signed replies dropped"
    );
    let echo = people
        .iter()
        .find(|m| m.subject == "Lunch on Friday")
        .unwrap();
    assert_eq!(echo.gist, None, "an echoed subject is not shown");
    let news = d.store().list(Bucket::Digest, None).unwrap();
    assert!(
        news.iter().all(|m| m.gist.is_some()),
        "newsletters in the latest and the coming edition get gists"
    );
    assert!(
        d.store().model_shops().unwrap().is_empty(),
        "the newsletter is a publication"
    );
    let e = d.edition(0).unwrap();
    assert!(
        e.headline.starts_with("So, "),
        "the model's headline: {}",
        e.headline
    );
    drop(d);
    assert_eq!(
        ai::pass(&hub, &Fake, model).await,
        0,
        "the rejected gist was retried once, then left alone"
    );
}
