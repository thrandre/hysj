use hysj_core::{Changes, RawHeaders, SENT_TAG, Store};
use hysj_proto::{Account, Bucket};

fn account(name: &str, address: &str) -> Account {
    Account {
        name: name.into(),
        address: address.into(),
        host: "imap.example.com".into(),
        port: 993,
        username: address.into(),
        smtp_host: "smtp.example.com".into(),
        smtp_port: 465,
        label: None,
    }
}

fn raw(remote_id: &str, from: &str, message_id: &str, tags: &[&str]) -> RawHeaders {
    RawHeaders {
        remote_id: remote_id.into(),
        headers: format!("From: {from}\r\nSubject: s{remote_id}\r\nMessage-ID: <{message_id}>\r\n")
            .into_bytes(),
        tags: tags.iter().map(|t| t.to_string()).collect(),
        provider_id: None,
    }
}

fn subjects(store: &Store, account: Option<&str>) -> Vec<String> {
    let mut s: Vec<_> = store
        .list(Bucket::Screener, account)
        .unwrap()
        .into_iter()
        .map(|m| m.subject)
        .collect();
    s.sort();
    s
}

#[test]
fn removals_tags_identities_dedupe() {
    let mut store = Store::open_in_memory().unwrap();
    store
        .add_account(&account("gmail", "Me@Gmail.com"))
        .unwrap();
    store
        .add_account(&account("work", "me@work.example"))
        .unwrap();

    let batch = Changes {
        cursor: "c1".into(),
        new: vec![
            raw("1", "a@x.example", "m1", &["\\Inbox"]),
            raw("2", "b@x.example", "m2", &[]),
            // Mail the user sent from an alias: becomes an identity, never listed.
            raw("3", "Me Alias <alias@me.example>", "m3", &[SENT_TAG]),
        ],
        ..Default::default()
    };
    assert_eq!(store.apply("gmail", &batch).unwrap(), 3);
    assert_eq!(subjects(&store, None), ["s1", "s2"]);
    let ids: Vec<_> = store
        .identities()
        .unwrap()
        .into_iter()
        .map(|i| i.address)
        .collect();
    assert_eq!(
        ids,
        ["alias@me.example", "me@gmail.com", "me@work.example"],
        "lower case"
    );

    // Mail from the account's own address is left out too, whatever its case.
    store
        .apply(
            "gmail",
            &Changes {
                new: vec![raw("4", "ME@gmail.com", "m4", &[])],
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(subjects(&store, None), ["s1", "s2"]);

    // Archive (label change) and deletion. "5" arrived after the server listed
    // what's present, so it is missing from `present` but must survive.
    let batch = Changes {
        retagged: vec![("1".into(), vec![])],
        present: Some(vec!["1".into(), "3".into(), "4".into()]),
        new: vec![raw("5", "c@x.example", "m5", &["\\Inbox"])],
        ..Default::default()
    };
    store.apply("gmail", &batch).unwrap();
    let list = store.list(Bucket::Screener, None).unwrap();
    let s1 = list.iter().find(|m| m.subject == "s1").unwrap();
    assert!(s1.tags.is_empty(), "archived: Inbox label gone");
    assert_eq!(subjects(&store, None), ["s1", "s5"], "s2 deleted, s5 kept");

    // Same message delivered to a second account: once in the unified view, present in each account's view.
    store
        .apply(
            "work",
            &Changes {
                new: vec![raw("w1", "a@x.example", "m1", &[])],
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(subjects(&store, None), ["s1", "s5"]);
    assert_eq!(subjects(&store, Some("work")), ["sw1"]);
    assert_eq!(subjects(&store, Some("gmail")), ["s1", "s5"]);
}

#[test]
fn migrates_a_v1_store() {
    let dir = std::env::temp_dir().join(format!("hysj-migrate-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("v1.db");
    let _ = std::fs::remove_file(&path);
    {
        // The step-2 schema, as it exists on users' disks (user_version 0).
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE TABLE accounts (name TEXT PRIMARY KEY, address TEXT NOT NULL, host TEXT NOT NULL, port INTEGER NOT NULL, username TEXT NOT NULL);
             CREATE TABLE messages (id INTEGER PRIMARY KEY, account TEXT NOT NULL REFERENCES accounts(name) ON DELETE CASCADE,
                remote_id TEXT NOT NULL, message_id TEXT, from_name TEXT, from_addr TEXT NOT NULL, subject TEXT NOT NULL,
                date INTEGER, bucket TEXT NOT NULL, reason TEXT NOT NULL, headers BLOB NOT NULL, UNIQUE (account, remote_id));
             CREATE TABLE bodies (message INTEGER PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE, text TEXT NOT NULL);
             CREATE TABLE sync_state (account TEXT PRIMARY KEY REFERENCES accounts(name) ON DELETE CASCADE, cursor TEXT NOT NULL);
             INSERT INTO accounts VALUES ('gmail', 'Me@Gmail.com', 'imap.gmail.com', 993, 'me');
             INSERT INTO messages VALUES (1, 'gmail', '9:1', 'm1', NULL, 'a@x.example', 'old', 0, 'screener', 'Not sorted yet', x'');",
        )
        .unwrap();
    }
    let store = Store::open(&path).unwrap();
    assert_eq!(
        subjects(&store, None),
        ["old"],
        "rows survive, tags default to []"
    );
    assert_eq!(
        store.identities().unwrap()[0].address,
        "me@gmail.com",
        "seeded from accounts"
    );
    drop(store);
    Store::open(&path).unwrap(); // reopening a migrated store is a no-op
    std::fs::remove_dir_all(&dir).unwrap();
}

fn sent(remote_id: &str, from: &str, to: &str) -> RawHeaders {
    RawHeaders {
        remote_id: remote_id.into(),
        headers: format!("From: {from}\r\nTo: {to}\r\n").into_bytes(),
        ..Default::default()
    }
}

fn bucket_of(store: &Store, subject: &str) -> Option<(Bucket, hysj_proto::Reason)> {
    [
        Bucket::People,
        Bucket::Digest,
        Bucket::Screener,
        Bucket::Blocked,
    ]
    .into_iter()
    .flat_map(|b| store.list(b, None).unwrap())
    .find(|m| m.subject == subject)
    .map(|m| (m.bucket, m.reason))
}

#[test]
fn sent_history_and_decisions() {
    use hysj_proto::{Decision, Reason};
    let mut store = Store::open_in_memory().unwrap();
    store
        .add_account(&account("gmail", "me@gmail.com"))
        .unwrap();
    store
        .add_account(&account("work", "me@work.example"))
        .unwrap();

    // Mail from a stranger to both accounts, and from Aisha.
    let inbox = |id: &str, from: &str, mid: &str| raw(id, from, mid, &[]);
    store
        .apply(
            "gmail",
            &Changes {
                new: vec![
                    inbox("1", "Aisha <aisha@x.example>", "m1"),
                    inbox("2", "jonas@y.example", "m2"),
                ],
                ..Default::default()
            },
        )
        .unwrap();
    store
        .apply(
            "work",
            &Changes {
                new: vec![inbox("w1", "jonas@y.example", "m3")],
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        bucket_of(&store, "s1"),
        Some((Bucket::Screener, Reason::FirstTime))
    );

    // Sent history: twice to Aisha, once to myself on the other account (a note-to-self
    // must not make me a "contact"), from an alias that becomes an identity.
    let history = Changes {
        sent: vec![
            sent("s-a", "Me <me@gmail.com>", "Aisha <AISHA@x.example>"),
            sent(
                "s-b",
                "Alias <alias@me.example>",
                "aisha@x.example, me@work.example",
            ),
        ],
        ..Default::default()
    };
    store.apply("gmail", &history).unwrap();
    assert_eq!(
        bucket_of(&store, "s1"),
        Some((Bucket::People, Reason::EmailedBefore { times: 2 }))
    );
    assert!(
        store
            .identities()
            .unwrap()
            .iter()
            .any(|i| i.address == "alias@me.example")
    );
    // Re-sending the same history is idempotent; a reset replaces it.
    store.apply("gmail", &history).unwrap();
    assert_eq!(
        bucket_of(&store, "s1"),
        Some((Bucket::People, Reason::EmailedBefore { times: 2 }))
    );
    store
        .apply(
            "gmail",
            &Changes {
                sent_reset: true,
                sent: vec![history.sent[0].clone()],
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        bucket_of(&store, "s1"),
        Some((Bucket::People, Reason::EmailedBefore { times: 1 }))
    );

    // One block, global: Jonas's mail leaves the Screener in both accounts.
    let (bucket, n) = store.decide(" Jonas@Y.example ", Decision::Block).unwrap();
    assert_eq!((bucket, n), (Bucket::Blocked, 2));
    assert!(store.list(Bucket::Screener, None).unwrap().is_empty());
    assert_eq!(store.list(Bucket::Blocked, Some("work")).unwrap().len(), 1);
    // ...and survives a resync of the account.
    store
        .apply(
            "work",
            &Changes {
                reset: true,
                new: vec![inbox("w1", "jonas@y.example", "m3")],
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(bucket_of(&store, "sw1").unwrap().0, Bucket::Blocked);
    // Changing your mind moves it back out.
    store.decide("jonas@y.example", Decision::Allow).unwrap();
    assert_eq!(store.list(Bucket::People, Some("work")).unwrap().len(), 1);
}

#[test]
fn done_survives_resync_and_covers_every_copy() {
    use hysj_proto::Intent;
    let mut store = Store::open_in_memory().unwrap();
    store
        .add_account(&account("gmail", "me@gmail.com"))
        .unwrap();
    store
        .add_account(&account("work", "me@work.example"))
        .unwrap();
    let msg = |id: &str, mid: &str| RawHeaders {
        remote_id: id.into(),
        headers: format!(
            "From: a@x.example\r\nTo: me@gmail.com\r\nSubject: s{id}\r\nMessage-ID: <{mid}>\r\n"
        )
        .into_bytes(),
        ..Default::default()
    };
    store
        .apply(
            "gmail",
            &Changes {
                new: vec![msg("1", "same"), msg("2", "other")],
                ..Default::default()
            },
        )
        .unwrap();
    store
        .apply(
            "work",
            &Changes {
                new: vec![msg("w1", "same")],
                ..Default::default()
            },
        )
        .unwrap();
    let screener = |s: &Store, a: Option<&str>| subjects(s, a);
    assert_eq!(screener(&store, None), ["s1", "s2"]);
    assert_eq!(
        store.list(Bucket::Screener, None).unwrap()[0].intent,
        Intent::Reply
    );

    let id = store
        .list(Bucket::Screener, Some("gmail"))
        .unwrap()
        .into_iter()
        .find(|m| m.subject == "s1")
        .unwrap()
        .id;
    assert_eq!(store.mark_done(&[id]).unwrap(), 1);
    assert_eq!(screener(&store, None), ["s2"]);
    assert!(
        screener(&store, Some("work")).is_empty(),
        "the other account's copy is cleared too"
    );

    // A resync deletes and re-inserts rows; the cleared message stays cleared.
    store
        .apply(
            "gmail",
            &Changes {
                reset: true,
                new: vec![msg("1", "same"), msg("2", "other")],
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(screener(&store, None), ["s2"]);

    // Replying (seen in sent history) moves it out of "Reply".
    let reply = RawHeaders {
        remote_id: "sent:1".into(),
        headers: b"From: me@gmail.com\r\nTo: a@x.example\r\nIn-Reply-To: <other>\r\n".to_vec(),
        ..Default::default()
    };
    store
        .apply(
            "gmail",
            &Changes {
                sent: vec![reply],
                ..Default::default()
            },
        )
        .unwrap();
    let s2 = store
        .list(Bucket::People, None)
        .unwrap()
        .into_iter()
        .find(|m| m.subject == "s2")
        .unwrap();
    assert_eq!((s2.intent, s2.replied), (Intent::Know, true));
}

#[test]
fn sending_context() {
    use hysj_core::store::reply_subject;
    let mut store = Store::open_in_memory().unwrap();
    store
        .add_account(&account("gmail", "me@gmail.com"))
        .unwrap();
    store
        .add_account(&account("work", "me@work.example"))
        .unwrap();

    // You wrote to Aisha from work, then later from an alias on gmail.
    let sent = |id: &str, from: &str, date: &str| RawHeaders {
        remote_id: id.into(),
        headers: format!("From: {from}\r\nTo: Aisha Rahman <aisha@x.example>\r\nDate: {date}\r\n")
            .into_bytes(),
        ..Default::default()
    };
    store
        .apply(
            "work",
            &Changes {
                sent: vec![sent(
                    "w",
                    "me@work.example",
                    "Mon, 5 Oct 2026 09:00:00 +0000",
                )],
                ..Default::default()
            },
        )
        .unwrap();
    store
        .apply(
            "gmail",
            &Changes {
                sent: vec![sent(
                    "g",
                    "Me <ALIAS@me.example>",
                    "Tue, 20 Oct 2026 09:00:00 +0000",
                )],
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        store
            .last_identity_for("AISHA@x.example ")
            .unwrap()
            .as_deref(),
        Some("alias@me.example"),
        "most recent wins"
    );
    assert_eq!(store.last_identity_for("nobody@x.example").unwrap(), None);
    assert_eq!(
        store
            .identity_account("Alias@me.example")
            .unwrap()
            .unwrap()
            .name,
        "gmail"
    );
    let c = store.contacts(10).unwrap();
    assert_eq!(
        (c[0].address.as_str(), c[0].name.as_deref(), c[0].times),
        ("aisha@x.example", Some("Aisha Rahman"), 2)
    );

    // A message to your work address, with Reply-To and a thread.
    let incoming = RawHeaders {
        remote_id: "1".into(),
        headers: b"From: Aisha Rahman <aisha@x.example>\r\nReply-To: team@x.example\r\nTo: me@work.example\r\nSubject: SV: Budsjett\r\nMessage-ID: <m2@x>\r\nReferences: <m0@x> <m1@x>\r\n".to_vec(),
        ..Default::default()
    };
    store
        .apply(
            "gmail",
            &Changes {
                new: vec![incoming],
                ..Default::default()
            },
        )
        .unwrap();
    let id = store.list(Bucket::People, None).unwrap()[0].id.clone();
    let summary = &store.list(Bucket::People, None).unwrap()[0];
    assert_eq!(
        summary.arrived_on, "me@work.example",
        "the address it was sent to, not the account's"
    );
    let ctx = store.reply_context(&id).unwrap().unwrap();
    assert_eq!(ctx.to, "team@x.example", "Reply-To wins");
    assert_eq!(
        ctx.subject, "SV: Budsjett",
        "no Re: on top of a Norwegian SV:"
    );
    assert_eq!(ctx.message_id.as_deref(), Some("m2@x"));
    assert_eq!(ctx.references, ["m0@x", "m1@x", "m2@x"]);
    assert_eq!(reply_subject("Lunsj"), "Re: Lunsj");
    assert_eq!(reply_subject("RE: Lunsj"), "RE: Lunsj");
}

#[test]
fn sent_and_done_folders() {
    let mut store = Store::open_in_memory().unwrap();
    store
        .add_account(&account("gmail", "me@gmail.com"))
        .unwrap();
    let msg = |id: &str, from: &str, to: &str| {
        RawHeaders {
        remote_id: id.into(),
        headers: format!("From: {from}\r\nTo: {to}\r\nSubject: s{id}\r\nMessage-ID: <{id}@x>\r\nDate: Mon, 19 Oct 2026 0{id}:00:00 +0000\r\n").into_bytes(),
        ..Default::default()
    }
    };
    let new = vec![
        msg("1", "a@x.example", "me@gmail.com"),
        msg(
            "2",
            "Me <me@gmail.com>",
            "Aisha Rahman <a@x.example>, b@x.example",
        ),
    ];
    store
        .apply(
            "gmail",
            &Changes {
                new,
                ..Default::default()
            },
        )
        .unwrap();

    let sent = store.sent(None, 10).unwrap();
    assert_eq!(sent.len(), 1, "only your own mail");
    assert_eq!(sent[0].to.as_deref(), Some("Aisha Rahman, b@x.example"));

    assert!(store.done(None, 10).unwrap().is_empty());
    let id = store.list(Bucket::Screener, None).unwrap()[0].id.clone();
    store.mark_done(std::slice::from_ref(&id)).unwrap();
    assert_eq!(store.done(None, 10).unwrap()[0].subject, "s1");
    assert!(store.list(Bucket::Screener, None).unwrap().is_empty());
    assert_eq!(store.restore(&[id]).unwrap(), 1);
    assert!(store.done(None, 10).unwrap().is_empty());
    assert_eq!(
        store.list(Bucket::Screener, None).unwrap().len(),
        1,
        "back where it was"
    );
}

#[test]
fn codes_bookings_orders_storage() {
    use hysj_core::extract::{Booking, Candidate, Code, CodeKind, Extract, Order};
    let mut store = Store::open_in_memory().unwrap();
    store
        .add_account(&account("gmail", "me@gmail.com"))
        .unwrap();
    let msg = |id: &str, subject: &str, date: &str| {
        RawHeaders {
        remote_id: id.into(),
        headers: format!("From: Svc <no-reply@svc.example>\r\nTo: me@gmail.com\r\nSubject: {subject}\r\nMessage-ID: <{id}@svc>\r\nDate: {date}\r\n").into_bytes(),
        ..Default::default()
    }
    };
    let new = vec![
        msg("c1", "Your sign-in code", "Mon, 19 Oct 2026 10:00:00 +0000"),
        msg("o1", "Takk for kjøpet", "Mon, 19 Oct 2026 11:00:00 +0000"),
        msg("n1", "Nyhetsbrev", "Mon, 19 Oct 2026 12:00:00 +0000"),
        msg(
            "b1",
            "Booking confirmation",
            "Tue, 20 Oct 2026 09:00:00 +0000",
        ),
        msg("b2", "Booking update", "Wed, 21 Oct 2026 09:00:00 +0000"),
    ];
    store
        .apply(
            "gmail",
            &Changes {
                new,
                ..Default::default()
            },
        )
        .unwrap();

    let pending = store.pending_extracts("gmail", 10).unwrap();
    let order: Vec<_> = pending.iter().map(|p| p.remote_id.as_str()).collect();
    assert_eq!(
        order,
        ["c1", "b2", "b1", "o1"],
        "codes first, then newest; newsletters skipped"
    );
    assert_eq!(pending[0].candidate, Candidate::Code);
    assert_eq!(
        store.pending_extracts("gmail", 2).unwrap().len(),
        2,
        "capped"
    );

    let code = Extract::Code(Code {
        kind: CodeKind::SignIn,
        service: "Svc".into(),
        code: Some("482913".into()),
        link: None,
        link_host: None,
        link_ok: false,
        sender_domain: "svc.example".into(),
        valid_minutes: 10,
    });
    store.record_extracts(&pending[0], &[code]).unwrap();
    assert!(
        store
            .list(Bucket::Digest, None)
            .unwrap()
            .iter()
            .all(|m| m.subject != "Your sign-in code"),
        "left the Digest"
    );

    // Valid for 10 minutes from the mail's Date (10:00 -> 10:10).
    let t = |hm: &str| hysj_core::store::iso_ts(&format!("2026-10-19T{hm}:00Z")).unwrap();
    let live = store.codes(t("10:05")).unwrap();
    assert_eq!(
        (live[0].code.as_deref(), live[0].expires),
        (Some("482913"), t("10:10"))
    );
    assert_eq!(
        store.codes(t("10:11")).unwrap()[0].code.as_deref(),
        Some("13"),
        "masked once expired"
    );
    assert!(
        store.codes(t("10:11") + 25 * 3600).unwrap().is_empty(),
        "gone after the 24 h retention"
    );

    // A booking moved by a later mail; both mails count as sources.
    let flight = |start: &str| {
        Extract::Booking(Booking {
            kind: "flight".into(),
            key: "flight:K7P2QX:XY123".into(),
            reference: "K7P2QX".into(),
            title: "Oslo OSL → Bergen BGO".into(),
            start: Some(start.into()),
            ..Default::default()
        })
    };
    store
        .record_extracts(&pending[2], &[flight("2026-11-06T07:15:00")])
        .unwrap();
    store
        .record_extracts(&pending[1], &[flight("2026-11-06T07:30:00")])
        .unwrap();
    let b = &store.bookings().unwrap()[0];
    assert_eq!(
        (b.start.as_deref(), b.moved_from.as_deref(), b.sources),
        (Some("2026-11-06T07:30:00"), Some("2026-11-06T07:15:00"), 2)
    );

    // Order: confirmation then a stale "processing" mail; status stays forward.
    let order = |status: u8, items: Vec<&str>| {
        Extract::Order(Order {
            key: "shop:1".into(),
            store: "Shop".into(),
            number: "1".into(),
            status,
            items: items.into_iter().map(String::from).collect(),
            ..Default::default()
        })
    };
    store
        .record_extracts(
            &pending[3],
            &[order(2, vec!["USB-C dock"]), order(1, vec![])],
        )
        .unwrap();
    let o = &store.orders().unwrap()[0];
    assert_eq!(
        (o.status, o.items.clone(), o.sources, o.receipt.is_some()),
        (2, vec!["USB-C dock".to_string()], 1, true)
    );

    // A resync re-inserts the rows: nothing is fetched again, codes stay out of the Digest.
    let again = vec![msg(
        "c1",
        "Your sign-in code",
        "Mon, 19 Oct 2026 10:00:00 +0000",
    )];
    store
        .apply(
            "gmail",
            &Changes {
                reset: true,
                new: again,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(store.pending_extracts("gmail", 10).unwrap().is_empty());
    assert!(store.list(Bucket::Digest, None).unwrap().is_empty());
}

#[test]
fn mails_with_a_booking_number_join_the_booking() {
    use hysj_core::extract::{ModelRead, checked_booking, checked_order};
    let mut store = Store::open_in_memory().unwrap();
    let payment = "Payment information. Booking 4815162. Total 392,00 SEK";
    let read = ModelRead {
        reference: "4815162",
        provider: "CompuSoft",
        total: "392,00 SEK",
        ..Default::default()
    };
    // The payment is read first and looks like an order…
    let o = checked_order(payment, &read).unwrap();
    store.record_order("pay@x", &o, 1).unwrap();
    assert_eq!(store.orders().unwrap().len(), 1);
    // …until the ticket with the same number arrives: one booking, three mails.
    let ticket = "Daftöland Ticket. Bestillingsnr 4815162. Från: 2026-10-03 16:00";
    let b = checked_booking(
        ticket,
        Some(1_790_000_000),
        &ModelRead {
            kind: "event",
            reference: "4815162",
            start: "2026-10-03T16:00",
            ..Default::default()
        },
    )
    .unwrap();
    store.record_booking("ticket@x", &b, 2).unwrap();
    assert!(store.orders().unwrap().is_empty(), "the payment folded in");
    assert!(store.link_booking("4815162", "confirm@x", 3).unwrap());
    assert!(!store.link_booking("999999", "other@x", 3).unwrap());
    let bookings = store.bookings().unwrap();
    assert_eq!((bookings.len(), bookings[0].sources), (1, 3));
}

#[test]
fn signals_need_an_authenticated_sender() {
    let mut store = Store::open_in_memory().unwrap();
    store
        .add_account(&account("gmail", "me@gmail.com"))
        .unwrap();
    let now = 1_790_000_000;
    let mail = |id: &str, from: &str, auth: &str, subject: &str| {
        RawHeaders {
        remote_id: id.into(),
        headers: format!(
            "{auth}From: {from}\r\nTo: me@gmail.com\r\nSubject: {subject}\r\nMessage-ID: <{id}@x>\r\nDate: Tue, 20 Oct 2026 16:55:41 +0000\r\n"
        )
        .into_bytes(),
        ..Default::default()
    }
    };
    let pass = "Authentication-Results: mx.google.com; dkim=pass header.i=@stripe.com; dmarc=pass header.from=stripe.com\r\n";
    let changes = Changes {
        new: vec![
            mail(
                "1",
                "Stripe <no-reply@stripe.com>",
                pass,
                "$20.00 payment to Example Cloud was unsuccessful",
            ),
            mail(
                "2",
                "Stripe <no-reply@stripe.com>",
                pass,
                "$20.00 payment to Example Cloud was unsuccessful again",
            ),
            // Same words, sender not verified: never on the Desk.
            mail(
                "3",
                "Bank <no-reply@bank.example>",
                "",
                "Your payment was declined",
            ),
            mail("4", "Stripe <no-reply@stripe.com>", pass, "Your receipt"),
        ],
        ..Default::default()
    };
    store.apply("gmail", &changes).unwrap();
    let s = store.signals(now).unwrap();
    assert_eq!(s.len(), 1, "{s:?}");
    assert_eq!(
        (s[0].kind.as_str(), s[0].ids.len()),
        ("payment", 2),
        "grouped by sender"
    );
    store.hold_signal("payment", true).unwrap();
    assert!(
        store.signals(now).unwrap().is_empty(),
        "held for the digest"
    );
    store.hold_signal("payment", false).unwrap();
    store.mark_done(&s[0].ids).unwrap();
    assert!(store.signals(now).unwrap().is_empty(), "done");
}

#[test]
fn a_pass_the_airline_attached_is_found() {
    use hysj_core::extract::{ModelRead, checked_booking};
    let mut store = Store::open_in_memory().unwrap();
    store
        .add_account(&account("gmail", "me@gmail.com"))
        .unwrap();
    let raw = "From: Airline <no-reply@air.example>\r\nTo: me@gmail.com\r\nSubject: Your booking K7P2QX\r\nMessage-ID: <p1@air.example>\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=X\r\n\r\n--X\r\nContent-Type: text/plain\r\n\r\nBooking K7P2QX, departs 2026-11-06 07:30.\r\n--X\r\nContent-Type: application/vnd.apple.pkpass; name=\"boarding.pkpass\"\r\nContent-Disposition: attachment; filename=\"boarding.pkpass\"\r\nContent-Transfer-Encoding: base64\r\n\r\nUEsDBA==\r\n--X--\r\n";
    let head = raw.split("\r\n\r\n").next().unwrap();
    store
        .apply(
            "gmail",
            &Changes {
                new: vec![RawHeaders {
                    remote_id: "1".into(),
                    headers: head.as_bytes().to_vec(),
                    ..Default::default()
                }],
                ..Default::default()
            },
        )
        .unwrap();
    let id = store.list(Bucket::Digest, None).unwrap()[0].id.clone();
    store.set_body(&id, "Booking K7P2QX").unwrap();
    store.set_raw(&id, raw.as_bytes()).unwrap();
    let b = checked_booking(
        raw,
        Some(1_790_000_000),
        &ModelRead {
            kind: "flight",
            reference: "K7P2QX",
            start: "2026-11-06T07:30",
            ..Default::default()
        },
    )
    .unwrap();
    store.record_booking("p1@air.example", &b, 1).unwrap();
    let pass = store.bookings().unwrap()[0].pass.clone();
    assert_eq!(pass.map(|p| (p.id, p.index)), Some((id, 0)));
}

#[test]
fn an_account_keeps_its_label() {
    let store = Store::open_in_memory().unwrap();
    store
        .add_account(&account("gmail-personal", "me@gmail.com"))
        .unwrap();
    assert!(
        store
            .set_account_label("gmail-personal", Some("Personal"))
            .unwrap()
    );
    // Changing servers re-adds the account; the label stays.
    store
        .add_account(&account("gmail-personal", "me@gmail.com"))
        .unwrap();
    assert_eq!(
        store.accounts().unwrap()[0].label.as_deref(),
        Some("Personal")
    );
    assert!(store.set_account_label("gmail-personal", None).unwrap());
    assert_eq!(store.accounts().unwrap()[0].label, None);
    assert!(!store.set_account_label("nope", Some("x")).unwrap());
}
