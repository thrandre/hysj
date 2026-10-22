//! Sending over SMTP (lettre). Message building is separate so it can be tested.

use std::hash::{BuildHasher, Hasher};

use hysj_core::{BackendError, Outgoing};
use lettre::message::{Mailbox, header::ContentType};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

pub(crate) type Smtp = AsyncSmtpTransport<Tokio1Executor>;

/// 465 = implicit TLS, anything else (587) = STARTTLS. TLS is never optional.
pub(crate) fn transport(
    host: &str,
    port: u16,
    username: &str,
    password: &str,
) -> Result<Smtp, BackendError> {
    let builder = if port == 465 {
        AsyncSmtpTransport::<Tokio1Executor>::relay(host)?
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)?
    };
    Ok(builder
        .port(port)
        .credentials(Credentials::new(username.into(), password.into()))
        .build())
}

pub(crate) async fn send(smtp: &Smtp, msg: &Message) -> Result<(), BackendError> {
    smtp.send(msg.clone()).await?;
    Ok(())
}

/// Builds the RFC 5322 message. Returns it with its Message-ID (without `<>`).
pub(crate) fn build(out: &Outgoing) -> Result<(Message, String), BackendError> {
    let from = Mailbox::new(out.from_name.clone(), out.from.parse()?);
    // Our own id: lettre's default embeds this machine's hostname.
    let domain = out.from.rsplit_once('@').map_or("hysj.invalid", |(_, d)| d);
    let random = std::hash::RandomState::new().build_hasher().finish();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let message_id = format!("{nanos:x}.{random:016x}@{domain}");

    let mut b = Message::builder()
        .from(from)
        .subject(out.subject.clone())
        .message_id(Some(format!("<{message_id}>")))
        .date_now();
    if out.to.is_empty() {
        return Err("no recipients".into());
    }
    for to in &out.to {
        for m in recipients(to)? {
            b = b.to(m);
        }
    }
    for cc in &out.cc {
        for m in recipients(cc)? {
            b = b.cc(m);
        }
    }
    if let Some(id) = &out.in_reply_to {
        b = b.in_reply_to(format!("<{id}>"));
    }
    if !out.references.is_empty() {
        let refs: Vec<String> = out.references.iter().map(|r| format!("<{r}>")).collect();
        b = b.references(refs.join(" "));
    }
    let msg = b.header(ContentType::TEXT_PLAIN).body(out.body.clone())?;
    Ok((msg, message_id))
}

/// `Name <a@b>, "Last, First" <c@d>, e@f`: split on commas outside quotes and
/// angle brackets, then build each mailbox from name + address, so real names
/// (`O'Neil (work)`, `Berg, Øystein`) needn't be valid RFC 5322 phrases.
fn recipients(list: &str) -> Result<Vec<Mailbox>, BackendError> {
    let mut parts = Vec::new();
    let (mut quoted, mut angle, mut start) = (false, false, 0);
    for (i, c) in list.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '<' if !quoted => angle = true,
            '>' if !quoted => angle = false,
            ',' if !quoted && !angle => {
                parts.push(&list[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&list[start..]);
    let mut out = Vec::new();
    for p in parts.into_iter().map(str::trim).filter(|p| !p.is_empty()) {
        let bad = |e: &dyn std::fmt::Display| format!("bad address {p:?}: {e}");
        out.push(match p.rsplit_once('<') {
            Some((name, addr)) => {
                let name = name.trim().trim_matches('"').replace("\\\"", "\"");
                let addr = addr
                    .trim_end_matches('>')
                    .trim()
                    .parse()
                    .map_err(|e| bad(&e))?;
                Mailbox::new((!name.is_empty()).then_some(name), addr)
            }
            None => Mailbox::new(None, p.parse().map_err(|e| bad(&e))?),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_threaded_reply() {
        let out = Outgoing {
            from: "me@example.com".into(),
            from_name: Some("Kari Nordmann".into()),
            to: vec!["Aisha Rahman <aisha@x.example>".into()],
            cc: vec!["jonas@y.example".into()],
            subject: "Re: Møte på torsdag".into(),
            body: "Ja, det passer.\n".into(),
            in_reply_to: Some("k1@x.example".into()),
            references: vec!["k0@x.example".into(), "k1@x.example".into()],
        };
        let (msg, id) = build(&out).unwrap();
        let text = String::from_utf8(msg.formatted()).unwrap();
        assert!(
            id.ends_with("@example.com"),
            "id on the sender's domain: {id}"
        );
        assert!(text.contains(&format!("Message-ID: <{id}>")));
        assert!(text.contains("In-Reply-To: <k1@x.example>"));
        assert!(text.contains("References: <k0@x.example> <k1@x.example>"));
        assert!(
            text.contains("To: \"Aisha Rahman\" <aisha@x.example>")
                || text.contains("To: Aisha Rahman <aisha@x.example>"),
            "{text}"
        );
        assert!(text.contains("Cc: jonas@y.example"));
        assert!(
            text.contains("Subject: Re: =?utf-8?b?"),
            "non-ASCII words are encoded: {text}"
        );
        assert!(!text.to_lowercase().contains(".local"), "no hostname leaks");
        assert!(
            build(&Outgoing {
                to: vec!["not an address".into()],
                ..out.clone()
            })
            .is_err()
        );
        assert!(build(&Outgoing { to: vec![], ..out }).is_err());
    }
}

#[cfg(test)]
mod names {
    use super::*;

    #[test]
    fn awkward_display_names() {
        for to in [
            "\"Berg, Øystein\" <o@x.example>",
            "Kari Nordmann <t@x.example>",
            "O'Neil (work) <n@x.example>",
            "Ærlig Åse <a@x.example>",
            "\"Berg, Øystein\" <o@x.example>, jonas@y.example",
        ] {
            let out = Outgoing {
                from: "me@example.com".into(),
                to: vec![to.into()],
                body: "x".into(),
                ..Default::default()
            };
            let (msg, _) = build(&out).unwrap_or_else(|e| panic!("{to}: {e}"));
            let text = String::from_utf8(msg.formatted()).unwrap();
            assert!(text.contains("@x.example>"), "{text}");
        }
        // Two recipients from one field, the name with a comma kept whole.
        let out = Outgoing {
            from: "me@example.com".into(),
            to: vec!["\"Berg, Øystein\" <o@x.example>, jonas@y.example".into()],
            body: "x".into(),
            ..Default::default()
        };
        let text = String::from_utf8(build(&out).unwrap().0.formatted()).unwrap();
        assert!(
            text.contains("o@x.example") && text.contains("jonas@y.example"),
            "{text}"
        );
    }
}
