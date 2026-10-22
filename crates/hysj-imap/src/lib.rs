//! IMAP backend: generic servers sync INBOX; Gmail syncs All Mail with labels
//! (see [`gmail`]).

mod gmail;
mod smtp;

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use async_imap::Session;
use async_imap::types::NameAttribute;
use futures_util::TryStreamExt;
use hysj_core::{BackendError, Changes, MailBackend, Outgoing, RawHeaders};
use hysj_proto::Account;
use rustls_platform_verifier::ConfigVerifierExt;
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::{ClientConfig, pki_types::ServerName};

/// Upper bound for any one operation, so a stalled server can't hang the daemon.
/// ponytail: one fixed value; make it scale with message size if big fetches need it.
const TIMEOUT: Duration = Duration::from_secs(120);
/// How far back the first sync of a mailbox reaches.
/// ponytail: fixed window; backfill older mail when idle once the scheduler exists (step 8).
const INITIAL_MESSAGES: usize = 500;

pub struct ImapBackend {
    session: Session<TlsStream<TcpStream>>,
    /// The one mailbox we sync: INBOX, or Gmail's All Mail.
    mailbox: String,
    /// Where sent mail lives; only its address headers are read.
    sent_mailbox: Option<String>,
    gmail: bool,
    condstore: bool,
    smtp: smtp::Smtp,
}

/// How much sent history the first sync reads (address headers only).
/// ponytail: most recent N; older history when idle, with the scheduler (step 8).
const SENT_HISTORY: usize = 5000;

/// SMTP host for an address, by the same convention as [`guess_host`].
pub fn guess_smtp_host(address: &str) -> String {
    guess_host(address).replacen("imap.", "smtp.", 1)
}

/// IMAP host for an address. ponytail: covers Gmail and the `imap.<domain>`
/// convention; ISPDB / RFC 6186 / MX autodiscovery arrives with the Accounts screen.
pub fn guess_host(address: &str) -> String {
    match address
        .rsplit_once('@')
        .map(|(_, d)| d.to_ascii_lowercase())
    {
        Some(d) if d == "gmail.com" || d == "googlemail.com" => "imap.gmail.com".into(),
        Some(d) => format!("imap.{d}"),
        None => String::new(),
    }
}

/// Sync position:
/// `<uidvalidity>:<floor>:<last uid>:<modseq>:<sent uidvalidity>:<sent last uid>:<mailbox>`.
/// `floor` is the lowest UID we ever synced (the initial window); the mailbox
/// goes last so a name containing `:` still parses.
#[derive(Debug, PartialEq)]
struct Cursor {
    validity: u32,
    floor: u32,
    last: u32,
    modseq: u64,
    sent_validity: u32,
    sent_last: u32,
    mailbox: String,
}

impl Cursor {
    fn parse(s: &str) -> Option<Self> {
        let mut p = s.splitn(7, ':');
        Some(Self {
            validity: p.next()?.parse().ok()?,
            floor: p.next()?.parse().ok()?,
            last: p.next()?.parse().ok()?,
            modseq: p.next()?.parse().ok()?,
            sent_validity: p.next()?.parse().ok()?,
            sent_last: p.next()?.parse().ok()?,
            mailbox: p.next()?.to_string(),
        })
    }

    fn format(&self) -> String {
        let Self {
            validity,
            floor,
            last,
            modseq,
            sent_validity,
            sent_last,
            mailbox,
        } = self;
        format!("{validity}:{floor}:{last}:{modseq}:{sent_validity}:{sent_last}:{mailbox}")
    }
}

/// RFC 6154 special-use mailboxes `(\All, \Sent)`. Names are localized
/// ("[Gmail]/Alle e-poster"), so go by attribute; common names as a fallback.
async fn special_use(
    session: &mut Session<TlsStream<TcpStream>>,
) -> Result<(Option<String>, Option<String>), BackendError> {
    let names: Vec<_> = session
        .list(Some(""), Some("*"))
        .await?
        .try_collect()
        .await?;
    let with = |attr: NameAttribute<'static>| {
        names
            .iter()
            .find(|n| n.attributes().contains(&attr))
            .map(|n| n.name().to_string())
    };
    let sent = with(NameAttribute::Sent).or_else(|| {
        names
            .iter()
            .map(|n| n.name())
            .find(|n| {
                ["sent", "sent items", "sent messages", "inbox.sent"]
                    .contains(&n.to_lowercase().as_str())
            })
            .map(str::to_string)
    });
    Ok((with(NameAttribute::All), sent))
}

/// Remote ids are `<uidvalidity>:<uid>` within the synced mailbox.
fn split_id(s: &str) -> Option<(u32, u32)> {
    let (v, u) = s.split_once(':')?;
    Some((v.parse().ok()?, u.parse().ok()?))
}

/// How to log in: a password, or an OAuth access token (Microsoft accounts,
/// which take no passwords; sent as SASL XOAUTH2).
#[derive(Clone, Copy)]
pub enum Login<'a> {
    Password(&'a str),
    Bearer(&'a str),
}

/// SASL XOAUTH2 (the client base64-encodes it).
struct XOAuth2<'a> {
    user: &'a str,
    token: &'a str,
}

impl async_imap::Authenticator for XOAuth2<'_> {
    type Response = String;
    fn process(&mut self, _challenge: &[u8]) -> String {
        format!("user={}\x01auth=Bearer {}\x01\x01", self.user, self.token)
    }
}

impl ImapBackend {
    /// Connects over implicit TLS, logs in and picks the mailbox to sync.
    pub async fn connect(account: &Account, login: Login<'_>) -> Result<Self, BackendError> {
        let mut backend = within(Self::connect_inner(
            &account.host,
            account.port,
            &account.username,
            login,
        ))
        .await?;
        // SMTP connects lazily, on the first send.
        backend.smtp = smtp::transport(
            &account.smtp_host,
            account.smtp_port,
            &account.username,
            login,
        )?;
        Ok(backend)
    }

    async fn connect_inner(
        host: &str,
        port: u16,
        username: &str,
        login: Login<'_>,
    ) -> Result<Self, BackendError> {
        let tcp = TcpStream::connect((host, port)).await?;
        // rustls + the OS trust store. (async-native-tls stalled every round trip on macOS.)
        // Both ring and aws-lc-rs are linked (mistral.rs pulls in the latter), so
        // rustls can't pick a default by itself: choose ring, once per process.
        let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();
        let config = ClientConfig::with_platform_verifier()?;
        let server = ServerName::try_from(host.to_string())?;
        let tls = tokio_rustls::TlsConnector::from(Arc::new(config))
            .connect(server, tcp)
            .await?;
        let mut client = async_imap::Client::new(tls);
        client
            .read_response()
            .await?
            .ok_or("server closed before greeting")?;
        let mut session = match login {
            Login::Password(password) => client.login(username, password).await,
            Login::Bearer(token) => {
                client
                    .authenticate(
                        "XOAUTH2",
                        XOAuth2 {
                            user: username,
                            token,
                        },
                    )
                    .await
            }
        }
        .map_err(|(e, _)| e)?;

        let caps = session.capabilities().await?;
        let gmail = caps.has_str("X-GM-EXT-1");
        let condstore = caps.has_str("CONDSTORE");
        let (all, sent_mailbox) = special_use(&mut session).await?;
        let mailbox = match (gmail, all) {
            (true, Some(all)) => all,
            (true, None) => "[Gmail]/All Mail".to_string(),
            (false, _) => "INBOX".to_string(),
        };
        Ok(Self {
            session,
            mailbox,
            sent_mailbox,
            gmail,
            condstore,
            // Replaced in `connect`; needs no network until used.
            smtp: smtp::transport(host, 465, username, login)?,
        })
    }

    /// New sent mail since `prev` (`(uidvalidity, last uid)` of the sent mailbox):
    /// Address and reply headers only. Returns (headers, history reset?, uidvalidity, last uid).
    async fn sent_since(
        &mut self,
        prev: (u32, u32),
    ) -> Result<(Vec<RawHeaders>, bool, u32, u32), BackendError> {
        let Some(sent) = self.sent_mailbox.clone() else {
            return Ok((Vec::new(), false, prev.0, prev.1));
        };
        let mb = self.session.select(&sent).await?;
        let validity = mb.uid_validity.ok_or("server sent no UIDVALIDITY")?;
        let reset = validity != prev.0;
        let after = if reset { 0 } else { prev.1 };
        if mb.exists == 0 {
            return Ok((Vec::new(), reset, validity, after));
        }
        let mut uids = self.uids(&format!("{}:*", after + 1)).await?;
        uids.retain(|&u| u > after);
        if reset {
            uids.drain(..uids.len().saturating_sub(SENT_HISTORY));
        }
        let last = uids.last().copied().unwrap_or(after);

        let mut out = Vec::with_capacity(uids.len());
        for chunk in uids.chunks(500) {
            let set = chunk
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(",");
            let fetches: Vec<_> = self
                .session
                .uid_fetch(
                    set,
                    "(UID BODY.PEEK[HEADER.FIELDS (FROM TO CC BCC DATE IN-REPLY-TO REFERENCES)])",
                )
                .await?
                .try_collect()
                .await?;
            out.extend(fetches.iter().filter_map(|f| {
                Some(RawHeaders {
                    remote_id: format!("sent:{validity}:{}", f.uid?),
                    headers: f.header()?.to_vec(),
                    ..Default::default()
                })
            }));
        }
        Ok((out, reset, validity, last))
    }

    /// Selects the mailbox: (UIDVALIDITY, message count, HIGHESTMODSEQ or 0).
    async fn select(&mut self) -> Result<(u32, u32, u64), BackendError> {
        let mb = if self.condstore {
            self.session.select_condstore(&self.mailbox).await?
        } else {
            self.session.select(&self.mailbox).await?
        };
        let validity = mb.uid_validity.ok_or("server sent no UIDVALIDITY")?;
        Ok((validity, mb.exists, mb.highest_modseq.unwrap_or(0)))
    }

    async fn uids(&mut self, range: &str) -> Result<Vec<u32>, BackendError> {
        let mut uids: Vec<u32> = self
            .session
            .uid_search(format!("UID {range}"))
            .await?
            .into_iter()
            .collect();
        uids.sort_unstable();
        Ok(uids)
    }

    async fn sync_inner(&mut self, cursor: Option<&str>) -> Result<Changes, BackendError> {
        // HIGHESTMODSEQ as of the start: label changes made during this sync get
        // picked up again next time rather than skipped.
        let (validity, exists, modseq) = self.select().await?;
        let prev = cursor
            .and_then(Cursor::parse)
            .filter(|c| c.validity == validity && c.mailbox == self.mailbox);
        let mut next = Cursor {
            validity,
            floor: prev.as_ref().map_or(1, |p| p.floor),
            last: prev.as_ref().map_or(0, |p| p.last),
            modseq,
            sent_validity: prev.as_ref().map_or(0, |p| p.sent_validity),
            sent_last: prev.as_ref().map_or(0, |p| p.sent_last),
            mailbox: self.mailbox.clone(),
        };

        let mut changes = if exists == 0 {
            Changes {
                reset: prev.is_none(),
                present: Some(Vec::new()),
                ..Default::default()
            }
        } else {
            self.mailbox_changes(prev.as_ref(), &mut next).await?
        };

        // Sent history last: it selects another mailbox. A fresh main cursor
        // re-reads the history too, so a reset never leaves it half-known.
        let sent_prev = match &prev {
            Some(_) => (next.sent_validity, next.sent_last),
            None => (0, 0),
        };
        let (sent, sent_reset, sent_validity, sent_last) = self.sent_since(sent_prev).await?;
        changes.sent = sent;
        changes.sent_reset = sent_reset;
        next.sent_validity = sent_validity;
        next.sent_last = sent_last;

        changes.cursor = next.format();
        Ok(changes)
    }

    /// New mail, label changes and deletions in the main mailbox (already selected, not empty).
    async fn mailbox_changes(
        &mut self,
        prev: Option<&Cursor>,
        next: &mut Cursor,
    ) -> Result<Changes, BackendError> {
        let validity = next.validity;
        let id = |uid: u32| format!("{validity}:{uid}");

        if prev.is_none() {
            let all = self.uids("1:*").await?;
            next.floor = *all
                .get(all.len().saturating_sub(INITIAL_MESSAGES))
                .ok_or("server listed messages but returned no UIDs")?;
            next.last = next.floor - 1;
        }

        // New messages.
        let query = if self.gmail {
            "(UID X-GM-MSGID X-GM-LABELS BODY.PEEK[HEADER])"
        } else {
            "(UID BODY.PEEK[HEADER])"
        };
        let first = next.last + 1;
        let fetches: Vec<_> = self
            .session
            .uid_fetch(format!("{first}:*"), query)
            .await?
            .try_collect()
            .await?;
        let mut new = Vec::new();
        for f in &fetches {
            // `n:*` always returns the newest message, even when its UID is below n.
            let (Some(uid), Some(headers)) = (f.uid, f.header()) else {
                continue;
            };
            if uid < first {
                continue;
            }
            next.last = next.last.max(uid);
            new.push(RawHeaders {
                remote_id: id(uid),
                headers: headers.to_vec(),
                tags: gmail::labels(f),
                provider_id: f.gmail_msg_id().map(u64::to_string),
            });
        }

        let mut changes = Changes {
            reset: prev.is_none(),
            new,
            ..Default::default()
        };
        if let Some(prev) = prev {
            // Label changes (archive = `\Inbox` removed) on already-synced mail.
            if self.gmail && prev.modseq > 0 && prev.last >= prev.floor {
                let changed: Vec<_> = self
                    .session
                    .uid_fetch(
                        format!("{}:{}", prev.floor, prev.last),
                        format!("(UID X-GM-LABELS) (CHANGEDSINCE {})", prev.modseq),
                    )
                    .await?
                    .try_collect()
                    .await?;
                changes.retagged = changed
                    .iter()
                    .filter_map(|f| Some((id(f.uid?), gmail::labels(f))))
                    .collect();
            }
            // Deletions: what's still there, searched *after* the new-mail fetch.
            // ponytail: one UID SEARCH per sync over our window; QRESYNC VANISHED where servers have it.
            let present = self.uids(&format!("{}:*", prev.floor)).await?;
            changes.present = Some(
                present
                    .into_iter()
                    .filter(|&u| u >= prev.floor)
                    .map(id)
                    .collect(),
            );
        }
        Ok(changes)
    }

    async fn fetch_inner(&mut self, remote_id: &str) -> Result<Vec<u8>, BackendError> {
        let (v, uid) = split_id(remote_id).ok_or("malformed remote id")?;
        if self.select().await?.0 != v {
            return Err("mailbox changed since sync; run sync again".into());
        }
        let fetches: Vec<_> = self
            .session
            .uid_fetch(uid.to_string(), "BODY.PEEK[]")
            .await?
            .try_collect()
            .await?;
        let body = fetches
            .iter()
            .find_map(|f| f.body())
            .ok_or("message no longer on server")?;
        Ok(body.to_vec())
    }
}

impl MailBackend for ImapBackend {
    // ponytail: no IDLE push yet (step 8); archive/delete via `apply` when the Desk can clear cards.
    async fn sync_since(&mut self, cursor: Option<&str>) -> Result<Changes, BackendError> {
        within(self.sync_inner(cursor)).await
    }

    async fn fetch(&mut self, remote_id: &str) -> Result<Vec<u8>, BackendError> {
        within(self.fetch_inner(remote_id)).await
    }

    async fn search(&mut self, query: &str, limit: usize) -> Result<Vec<RawHeaders>, BackendError> {
        within(async {
            let validity = self.select().await?.0;
            // A quoted IMAP string: no line breaks, \ and " escaped.
            let q: String = query
                .chars()
                .filter(|c| !c.is_control())
                .collect::<String>()
                .replace('\\', "\\\\")
                .replace('"', "\\\"");
            let criteria = if self.gmail {
                format!("CHARSET UTF-8 X-GM-RAW \"{q}\"")
            } else {
                format!("CHARSET UTF-8 TEXT \"{q}\"")
            };
            let mut uids: Vec<u32> = self
                .session
                .uid_search(criteria)
                .await?
                .into_iter()
                .collect();
            uids.sort_unstable_by(|a, b| b.cmp(a));
            uids.truncate(limit);
            if uids.is_empty() {
                return Ok(Vec::new());
            }
            let set = uids
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(",");
            let query = if self.gmail {
                "(UID X-GM-LABELS BODY.PEEK[HEADER])"
            } else {
                "(UID BODY.PEEK[HEADER])"
            };
            let fetches: Vec<_> = self
                .session
                .uid_fetch(set, query)
                .await?
                .try_collect()
                .await?;
            let mut out: Vec<RawHeaders> = fetches
                .iter()
                .filter_map(|f| {
                    Some(RawHeaders {
                        remote_id: format!("{validity}:{}", f.uid?),
                        headers: f.header()?.to_vec(),
                        tags: gmail::labels(f),
                        provider_id: None,
                    })
                })
                .collect();
            out.sort_by_key(|r| std::cmp::Reverse(split_id(&r.remote_id).map(|x| x.1)));
            Ok(out)
        })
        .await
    }

    async fn fetch_many(
        &mut self,
        remote_ids: &[String],
        max_bytes: u32,
    ) -> Result<Vec<(String, Vec<u8>)>, BackendError> {
        within(async {
            let validity = self.select().await?.0;
            let uids: Vec<u32> = remote_ids
                .iter()
                .filter_map(|r| split_id(r))
                .filter(|(v, _)| *v == validity)
                .map(|(_, u)| u)
                .collect();
            if uids.is_empty() {
                return Ok(Vec::new());
            }
            let set = uids
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(",");
            let fetches: Vec<_> = self
                .session
                .uid_fetch(set, format!("(UID BODY.PEEK[]<0.{max_bytes}>)"))
                .await?
                .try_collect()
                .await?;
            Ok(fetches
                .iter()
                .filter_map(|f| Some((format!("{validity}:{}", f.uid?), f.body()?.to_vec())))
                .collect())
        })
        .await
    }

    /// Once SMTP has accepted the message it counts as sent: a failure to file
    /// the copy afterwards is logged, never reported (that would invite a resend).
    async fn send(&mut self, out: &Outgoing) -> Result<String, BackendError> {
        let (msg, id) = smtp::build(out)?;
        within(smtp::send(&self.smtp, &msg)).await?;
        // Gmail files what its SMTP sends; appending would duplicate it.
        if !self.gmail
            && let Some(sent) = self.sent_mailbox.clone()
        {
            let filed = within(async {
                self.session
                    .append(&sent, Some("(\\Seen)"), None, msg.formatted())
                    .await?;
                Ok(())
            })
            .await;
            if let Err(e) = filed {
                tracing::warn!(error = %e, "sent, but filing a copy in Sent failed");
            }
        }
        Ok(id)
    }
}

async fn within<T>(f: impl Future<Output = Result<T, BackendError>>) -> Result<T, BackendError> {
    tokio::time::timeout(TIMEOUT, f)
        .await
        .map_err(|_| format!("mail server did not answer within {}s", TIMEOUT.as_secs()))?
}

#[cfg(test)]
mod xoauth2_tests {
    use async_imap::Authenticator;

    #[test]
    fn the_line_microsoft_expects() {
        let mut a = super::XOAuth2 {
            user: "kari@hotmail.com",
            token: "T0KEN",
        };
        assert_eq!(
            a.process(b""),
            "user=kari@hotmail.com\x01auth=Bearer T0KEN\x01\x01"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_cursors_ids() {
        assert_eq!(guess_host("kai@Gmail.com"), "imap.gmail.com");
        assert_eq!(guess_host("kari@example.no"), "imap.example.no");
        let c = Cursor {
            validity: 7,
            floor: 100,
            last: 250,
            modseq: 9001,
            sent_validity: 3,
            sent_last: 42,
            mailbox: "[Gmail]/All: Mail".into(),
        };
        assert_eq!(Cursor::parse(&c.format()), Some(c));
        assert_eq!(
            Cursor::parse("7:100:250:9001:[Gmail]/All Mail"),
            None,
            "step-3 cursors are rejected, forcing a resync"
        );
        assert_eq!(
            Cursor::parse("123:45"),
            None,
            "step-2 cursors are rejected, forcing a resync"
        );
        assert_eq!(split_id("123:45"), Some((123, 45)));
        assert_eq!(split_id("nope"), None);
    }
}
