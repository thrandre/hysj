//! Extraction rules against synthetic mails (fixture codes only).

use hysj_core::extract::{Candidate, CodeKind, Extract, candidate, extract};

const AR_OK: &str = "Authentication-Results: mx.google.com;\r\n dkim=pass header.i=@service.example header.s=s1;\r\n spf=pass smtp.mailfrom=service.example;\r\n dmarc=pass (p=REJECT) header.from=service.example\r\n";

fn mail(auth: &str, from: &str, subject: &str, text: &str) -> Vec<u8> {
    format!("{auth}From: {from}\r\nTo: me@example.com\r\nSubject: {subject}\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{text}\r\n").into_bytes()
}

fn code_of(raw: &[u8]) -> Option<(CodeKind, Option<String>, u32)> {
    extract(raw).into_iter().find_map(|e| match e {
        Extract::Code(c) => Some((c.kind, c.code, c.valid_minutes)),
        _ => None,
    })
}

const SVC: &str = "Service <no-reply@service.example>";

#[test]
fn finds_codes() {
    let cases = [
        (
            "Your sign-in code",
            "Your code is 482913. It expires in 15 minutes.",
            CodeKind::SignIn,
            "482913",
            15,
        ),
        (
            "Engangskode",
            "Engangskoden din er 482 913.\nGyldig i 5 minutter.",
            CodeKind::Code,
            "482913",
            5,
        ),
        (
            "Bekreft e-posten din",
            "Bruk koden 4821-9031 for å bekrefte.",
            CodeKind::Verification,
            "48219031",
            10,
        ),
        (
            "Logg inn",
            "Koden din: 4821 Hilsen Service",
            CodeKind::SignIn,
            "4821",
            10,
        ),
    ];
    for (subject, text, kind, digits, minutes) in cases {
        let got = code_of(&mail(AR_OK, SVC, subject, text));
        assert_eq!(got, Some((kind, Some(digits.into()), minutes)), "{subject}");
    }
}

#[test]
fn ignores_numbers_that_are_not_codes() {
    let cases = [
        ("Din rabattkode", "Bruk rabattkode 4821 i kassen."),
        (
            "Ny kode for parkering",
            "Besøk oss i Storgata 4, 1607 Fredrikstad.",
        ),
        (
            "Kode til kampanjen",
            "Nå kun 1299 kr! Gjelder til 2026-10-31.",
        ),
        ("Login help", "Call us at +47 91234567 or tlf 91234567."),
        ("Verifisering av ordre", "Ordre 12345678 er mottatt."),
        ("Your code", "Order #48291 shipped on 21.10.2026 at 14:05."),
        (
            "Security code update",
            "Since 2019 we have protected your account.",
        ),
    ];
    for (subject, text) in cases {
        assert_eq!(code_of(&mail(AR_OK, SVC, subject, text)), None, "{subject}");
    }
}

#[test]
fn only_trusts_authenticated_senders() {
    let text = "Your code is 482913.";
    assert_eq!(
        code_of(&mail("", SVC, "Your code", text)),
        None,
        "no Authentication-Results"
    );
    let fail = "Authentication-Results: mx.google.com; dkim=fail header.i=@service.example; dmarc=fail header.from=service.example\r\n";
    assert_eq!(
        code_of(&mail(fail, SVC, "Your code", text)),
        None,
        "failed DKIM/DMARC"
    );
    let other = "Authentication-Results: mx.google.com; dkim=pass header.i=@attacker.example; dmarc=pass header.from=attacker.example\r\n";
    assert_eq!(
        code_of(&mail(other, SVC, "Your code", text)),
        None,
        "signed by another domain"
    );
    // A sender-supplied header below ours doesn't count.
    let forged = format!("Authentication-Results: mx.google.com; dkim=none; dmarc=none\r\n{AR_OK}");
    assert_eq!(
        code_of(&mail(&forged, SVC, "Your code", text)),
        None,
        "only the topmost header"
    );
    // Subdomain signing aligns (relaxed alignment).
    let sub = "Authentication-Results: mx.google.com; dkim=pass header.d=mail.service.example\r\n";
    assert!(code_of(&mail(sub, SVC, "Your code", text)).is_some());
}

#[test]
fn reset_links() {
    let html = |href: &str| {
        format!(
        "{AR_OK}From: {SVC}\r\nSubject: Reset your password\r\nMIME-Version: 1.0\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<p>Click <a href=\"{href}\">here</a>. The link is valid for 1 hour.</p>\r\n"
    ).into_bytes()
    };
    let reset = |raw: &[u8]| {
        extract(raw).into_iter().find_map(|e| match e {
            Extract::Code(c) => Some(c),
            _ => None,
        })
    };
    let own = reset(&html("https://accounts.service.example/reset?token=abc")).unwrap();
    assert_eq!(
        (own.kind, own.link_ok, own.valid_minutes, own.code),
        (CodeKind::Reset, true, 60, None)
    );
    let tracked = reset(&html(
        "https://click.mailer.example/ls/click?upn=reset-password",
    ))
    .unwrap();
    assert_eq!(
        (tracked.link_ok, tracked.link_host.as_deref()),
        (false, Some("click.mailer.example"))
    );
}

fn html_mail(subject: &str, json: &str) -> Vec<u8> {
    format!(
        "From: Shop <orders@shop.example>\r\nSubject: {subject}\r\nMIME-Version: 1.0\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<html><head><script data-x='1' type='application/ld+json'>{json}</script></head><body>Hi</body></html>\r\n"
    )
    .into_bytes()
}

#[test]
fn schema_org_bookings() {
    let flight = r#"{"@context":"https://schema.org","@graph":[{"@type":["https://schema.org/FlightReservation"],"reservationNumber":"K7P2QX",
        "reservationFor":{"@type":"Flight","flightNumber":"123","airline":{"name":"Air","iataCode":"XY"},
        "departureAirport":{"name":"Oslo","iataCode":"OSL"},"arrivalAirport":{"iataCode":"BGO","address":{"addressLocality":"Bergen"}},
        "departureTime":"2026-11-06T07:30:00","arrivalTime":"2026-11-06T08:25:00"},
        "reservedTicket":{"ticketedSeat":{"seatNumber":"12A"}}},
        {"@type":"LodgingReservation","reservationNumber":"48210937","reservationFor":{"name":"Hotel Bryggen","address":{"addressLocality":"Bergen"}},
        "checkinTime":"2026-11-06T15:00:00+01:00","checkoutTime":"2026-11-08T11:00:00+01:00"}]}"#;
    let got: Vec<_> = extract(&html_mail("Booking confirmation", flight))
        .into_iter()
        .filter_map(|e| match e {
            Extract::Booking(b) => Some(b),
            _ => None,
        })
        .collect();
    assert_eq!(got.len(), 2);
    assert_eq!(
        (
            got[0].kind.as_str(),
            got[0].key.as_str(),
            got[0].title.as_str()
        ),
        ("flight", "flight:K7P2QX:XY123", "Oslo OSL → Bergen BGO")
    );
    assert_eq!(
        (
            got[0].start.as_deref(),
            got[0].seat.as_deref(),
            got[0].city.as_deref()
        ),
        (Some("2026-11-06T07:30:00"), Some("12A"), Some("Bergen"))
    );
    assert_eq!(
        (got[1].kind.as_str(), got[1].title.as_str()),
        ("hotel", "Hotel Bryggen")
    );

    // HTML-escaped JSON still parses.
    let escaped = r#"{&quot;@type&quot;:&quot;FoodEstablishmentReservation&quot;,&quot;reservationNumber&quot;:&quot;R-2291&quot;,&quot;partySize&quot;:4,&quot;startTime&quot;:&quot;2026-11-14T19:00:00&quot;,&quot;reservationFor&quot;:{&quot;name&quot;:&quot;Bistro&quot;}}"#;
    let table = extract(&html_mail("Reservation", escaped));
    assert!(matches!(&table[..], [Extract::Booking(b)] if b.kind == "table" && b.party == Some(4)));
}

#[test]
fn schema_org_orders_and_truncation() {
    let order = r#"[{"@type":"Order","orderNumber":"A-1","merchant":{"name":"Hardware"},"orderDate":"2026-10-17",
        "orderStatus":"https://schema.org/OrderProcessing","acceptedOffer":[{"itemOffered":{"name":"Oak board"},"eligibleQuantity":{"value":4}},{"itemOffered":{"name":"Wood glue"}}],
        "price":"899","priceCurrency":"NOK"},
        {"@type":"ParcelDelivery","trackingUrl":"https://track.example/1","expectedArrivalUntil":"2026-10-23",
        "partOfOrder":{"@type":"Order","orderNumber":"A-1","merchant":{"name":"Hardware"}}}]"#;
    let got: Vec<_> = extract(&html_mail("Order confirmation", order))
        .into_iter()
        .filter_map(|e| match e {
            Extract::Order(o) => Some(o),
            _ => None,
        })
        .collect();
    assert_eq!(got.len(), 2);
    assert_eq!(
        (
            got[0].key.as_str(),
            got[0].items.clone(),
            got[0].total.as_deref(),
            got[0].status
        ),
        (
            "hardware:A-1",
            vec!["Oak board ×4".to_string(), "Wood glue".into()],
            Some("NOK 899"),
            1
        )
    );
    assert_eq!(
        (got[1].key.as_str(), got[1].status, got[1].eta.as_deref()),
        ("hardware:A-1", 2, Some("2026-10-23"))
    );

    // A body cut off mid-JSON yields nothing rather than an error.
    let raw = html_mail("Order confirmation", order);
    assert!(extract(&raw[..raw.len() / 2]).is_empty());
}

#[test]
fn candidates() {
    assert_eq!(candidate("Your sign-in code"), Some(Candidate::Code));
    assert_eq!(candidate("Tilbakestill passord"), Some(Candidate::Code));
    assert_eq!(candidate("Takk for kjøpet!"), Some(Candidate::Structured));
    assert_eq!(
        candidate("Rabattkode: 20% på alt"),
        None,
        "marketing 'code'"
    );
    assert_eq!(candidate("Nyhetsbrev oktober"), None);
    // Code words count as words, not parts of other words.
    for s in [
        "[CodePen Spark] Neon Ghosts",
        "Codex is out",
        "Se hvordan du koder raskere",
        "Små tilbakestillinger i livet",
    ] {
        assert_eq!(candidate(s), None, "{s}");
    }
    for s in [
        "Din engangskode",
        "Koden din",
        "Verify your email",
        "Your verification codes",
        "Innloggingskode for Example",
    ] {
        assert_eq!(candidate(s), Some(Candidate::Code), "{s}");
    }
    // …and in the body: "CodePen" before a number doesn't make it a code.
    let raw = mail(
        AR_OK,
        "Example <hello@service.example>",
        "Verify your email",
        "Join 48291 makers on CodePen 48291 today.",
    );
    assert_eq!(code_of(&raw), None);
}
