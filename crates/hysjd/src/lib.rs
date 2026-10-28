//! The daemon: owns the store, the mail server connections and (later) the
//! models and scheduler. Clients talk to it over a [`Transport`] with JSON-RPC 2.0.
//!
//! Runs as the `hysjd` process (see `main.rs`); clients reach it over a Unix
//! socket ([`socket`]). Tests run it in-process via [`serve`].

pub mod ai;
pub mod kinds;
pub mod oauth;
pub mod provider;
pub mod schedule;
pub mod socket;

/// Argument a client gives its own executable to run it as the daemon, so the
/// daemon is always the build you just ran (no separate binary to keep in sync).
pub const DAEMON_FLAG: &str = "--daemon";

/// Runs the daemon until the process ends: store, scheduler, socket.
pub async fn run_daemon() -> anyhow::Result<()> {
    let store = Store::open(&default_db_path()?)?;
    let hub = Hub::new(Daemon::new(store));
    tokio::spawn(schedule::watch_exe());
    let h = hub.clone();
    tokio::spawn(schedule::supervise("scheduler", move || {
        schedule::run(h.clone())
    }));
    let h = hub.clone();
    tokio::spawn(schedule::supervise("model", move || ai::run(h.clone())));
    socket::listen(&socket::socket_path()?, hub).await?;
    Ok(())
}

/// Where model weights are downloaded (mistral.rs reads `$HF_HOME`). Call
/// before any runtime or thread exists: changing the environment is only
/// sound while the process is single-threaded.
pub fn set_models_dir() {
    if std::env::var_os("HF_HOME").is_none()
        && let Ok(db) = default_db_path()
    {
        // SAFETY: called at the top of `main`, before other threads start.
        unsafe { std::env::set_var("HF_HOME", db.with_file_name("models")) };
    }
}

/// Call first in a client's `main`: if this process was started as the
/// daemon, run it and exit instead of returning.
pub fn run_if_daemon() {
    if std::env::args().nth(1).as_deref() == Some(DAEMON_FLAG) {
        daemon_main();
    }
}

/// Logs to stderr (the daemon log), runs the daemon, exits with its status.
/// Set when this process is the daemon (not an in-process test hub).
static IS_DAEMON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn daemon_main() -> ! {
    IS_DAEMON.store(true, Ordering::Relaxed);
    set_models_dir();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let code = match rt.block_on(run_daemon()) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = %e, "daemon stopped");
            1
        }
    };
    std::process::exit(code);
}

use std::collections::HashMap;
use std::fmt::Display;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use hysj_core::{MailBackend, Outgoing, Store, Transport, sync};
use hysj_imap::{ImapBackend, Login};
use hysj_proto::MessageView;
use hysj_proto::{
    Account, AccountAddParams, AttachmentData, AttachmentParams, CodeReport, CodeSettings, Folder,
    FolderParams, Hello, HelloResult, IdentitySuggest, IdentitySuggestion, ListParams, Message,
    MessageDone, Notification, PROTOCOL_VERSION, Request, Response, RpcError, ScreenerDecide,
    ScreenerDecided, ScreenerForget, SendParams, SendResult, SenderImages, ShowParams,
    StateChanged, SyncParams, SyncResult, method,
};
use serde::de::DeserializeOwned;
use serde_json::Value;

const KEYRING_SERVICE: &str = "hysj";
/// Before the rename to Hysj; read once, moved to [`KEYRING_SERVICE`].
const OLD_KEYRING_SERVICE: &str = "quiet";

/// The account password from the keychain, moving it from the pre-rename
/// entry on first use.
pub fn password(address: &str) -> Result<String, RpcError> {
    let err = |e: keyring::Error| backend_err(format!("keychain: {e}"));
    let entry = keyring::Entry::new(KEYRING_SERVICE, address).map_err(err)?;
    match entry.get_password() {
        Err(keyring::Error::NoEntry) => {
            let old = keyring::Entry::new(OLD_KEYRING_SERVICE, address).map_err(err)?;
            let password = old.get_password().map_err(err)?;
            entry.set_password(&password).map_err(err)?;
            let _ = old.delete_credential();
            tracing::info!("moved the account password to the hysj keychain entry");
            Ok(password)
        }
        other => other.map_err(err),
    }
}

/// `$HYSJ_DB`, else `<data dir>/hysj/hysj.db` (created owner-only). A store
/// from before the rename (`<data dir>/quiet/quiet.db`) is moved over once.
pub fn default_db_path() -> std::io::Result<PathBuf> {
    if let Some(p) = std::env::var_os("HYSJ_DB") {
        return Ok(p.into());
    }
    let base = dirs::data_dir().ok_or(std::io::ErrorKind::NotFound)?;
    let dir = base.join("hysj");
    let old = base.join("quiet");
    if !dir.exists() && old.join("quiet.db").exists() {
        std::fs::rename(&old, &dir)?;
        for ext in ["", "-wal", "-shm"] {
            let from = dir.join(format!("quiet.db{ext}"));
            if from.exists() {
                std::fs::rename(from, dir.join(format!("hysj.db{ext}")))?;
            }
        }
        for stale in ["quietd.sock", "quietd.log"] {
            let _ = std::fs::remove_file(dir.join(stale));
        }
    }
    std::fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(dir.join("hysj.db"))
}

pub struct Daemon {
    store: Store,
    /// Open IMAP sessions by account name, connected on first use.
    sessions: HashMap<String, ImapBackend>,
    /// Passwords read from the keychain ahead of time, by address, so a
    /// keychain dialog never holds the daemon's lock (see `schedule`). For a
    /// Microsoft account it's the refresh token (`oauth:ms:…`).
    passwords: HashMap<String, String>,
    /// Microsoft access tokens by address (memory only; renewed when stale).
    tokens: HashMap<String, oauth::Tokens>,
}

impl Daemon {
    pub fn new(store: Store) -> Self {
        hysj_core::digest::set_hours(&store.digest_hours().unwrap_or_default());
        Self {
            store,
            sessions: HashMap::new(),
            passwords: HashMap::new(),
            tokens: HashMap::new(),
        }
    }

    /// Addresses of accounts whose password hasn't been read yet.
    pub(crate) fn passwords_missing(&self) -> Vec<String> {
        self.store
            .accounts()
            .unwrap_or_default()
            .into_iter()
            .map(|a| a.address)
            .filter(|a| !self.passwords.contains_key(a))
            .collect()
    }

    pub(crate) fn remember_password(&mut self, address: String, password: String) {
        self.passwords.insert(address, password);
    }

    /// Read access to the store (tests, tools).
    pub fn store(&self) -> &Store {
        &self.store
    }

    fn account(&self, name: &str) -> Result<Account, RpcError> {
        self.store
            .accounts()
            .map_err(backend_err)?
            .into_iter()
            .find(|a| a.name == name)
            .ok_or_else(|| RpcError::new(RpcError::NOT_FOUND, format!("no account named {name}")))
    }

    /// A fresh access token for an OAuth account, renewed if it's stale.
    /// Providers (Microsoft) may send a new refresh token each time; it's kept
    /// in memory at once and written to the keychain off the lock.
    async fn access_token(
        &mut self,
        address: &str,
        cfg: &'static oauth::OAuthConfig,
        refresh: &str,
    ) -> Result<String, RpcError> {
        if let Some(t) = self.tokens.get(address).filter(|t| t.fresh()) {
            return Ok(t.access.clone());
        }
        let t = oauth::refresh(cfg, refresh)
            .await
            .map_err(|e| backend_err(format!("{} sign-in: {e}", cfg.name)))?;
        if let Some(new) = t.refresh.clone().filter(|n| n != refresh) {
            let secret = oauth::secret(cfg, &new);
            self.passwords.insert(address.to_string(), secret.clone());
            let a = address.to_string();
            tokio::task::spawn_blocking(move || {
                if let Err(e) =
                    keyring::Entry::new(KEYRING_SERVICE, &a).and_then(|e| e.set_password(&secret))
                {
                    tracing::warn!(error = %e, "couldn't keep the renewed Microsoft sign-in");
                }
            });
        }
        let access = t.access.clone();
        self.tokens.insert(address.to_string(), t);
        Ok(access)
    }

    async fn session(&mut self, account: &Account) -> Result<&mut ImapBackend, RpcError> {
        // An OAuth session logged in with a token that's about to expire
        // reconnects (its SMTP login carries the token too).
        let oauth = self
            .passwords
            .get(&account.address)
            .is_some_and(|p| oauth::parse_secret(p).is_some());
        if oauth && !self.tokens.get(&account.address).is_some_and(|t| t.fresh()) {
            self.sessions.remove(&account.name);
        }
        if !self.sessions.contains_key(&account.name) {
            // Read by the scheduler, never here: a keychain dialog must not hold
            // the lock (see `schedule`).
            let password = self
                .passwords
                .get(&account.address)
                .cloned()
                .ok_or_else(|| {
                    backend_err(
                        "keychain: waiting for access — allow Hysj in the keychain dialog"
                            .to_string(),
                    )
                })?;
            let backend = match oauth::parse_secret(&password) {
                Some((cfg, refresh)) => {
                    let token = self.access_token(&account.address, cfg, refresh).await?;
                    ImapBackend::connect(account, Login::Bearer(&token)).await
                }
                None => ImapBackend::connect(account, Login::Password(&password)).await,
            }
            .map_err(backend_err)?;
            self.sessions.insert(account.name.clone(), backend);
        }
        Ok(self
            .sessions
            .get_mut(&account.name)
            .expect("inserted above"))
    }

    async fn add_account(&mut self, p: AccountAddParams) -> Result<Account, RpcError> {
        if p.name.is_empty() || !p.address.contains('@') {
            return Err(RpcError::new(
                RpcError::INVALID_PARAMS,
                "need a name and an email address",
            ));
        }
        // An existing account: its servers change, its address doesn't.
        if let Ok(old) = self.account(&p.name)
            && !old.address.eq_ignore_ascii_case(&p.address)
        {
            return Err(RpcError::new(
                RpcError::INVALID_PARAMS,
                format!("account {} is {}", p.name, old.address),
            ));
        }
        // Servers the person didn't type come from the provider registry.
        let known = provider::discover(&p.address);
        let account = Account {
            host: p.host.unwrap_or(known.imap_host),
            port: p.port.unwrap_or(known.imap_port),
            username: p.username.unwrap_or_else(|| p.address.clone()),
            smtp_host: p.smtp_host.unwrap_or(known.smtp_host),
            smtp_port: p.smtp_port.unwrap_or(known.smtp_port),
            // Kept as it is when an existing account's servers change.
            label: None,
            name: p.name,
            address: p.address,
        };
        // Only keep credentials that actually log in.
        let backend = ImapBackend::connect(&account, Login::Password(&p.password))
            .await
            .map_err(backend_err)?;
        keyring::Entry::new(KEYRING_SERVICE, &account.address)
            .and_then(|e| e.set_password(&p.password))
            .map_err(|e| backend_err(format!("keychain: {e}")))?;
        self.store.add_account(&account).map_err(backend_err)?;
        self.passwords
            .insert(account.address.clone(), p.password.clone());
        self.sessions.insert(account.name.clone(), backend);
        tracing::info!(account = %account.name, "account added");
        Ok(account)
    }

    /// Adds (or signs in again) an account after "Sign in with …": logs in
    /// with the token first, keeps nothing that doesn't work.
    async fn add_oauth(
        &mut self,
        address: &str,
        cfg: &'static oauth::OAuthConfig,
        t: oauth::Tokens,
    ) -> Result<Account, RpcError> {
        let refresh = t
            .refresh
            .clone()
            .ok_or_else(|| backend_err(format!("{} sent no refresh token", cfg.name)))?;
        let known = provider::discover(address);
        let existing = self
            .store
            .accounts()
            .map_err(backend_err)?
            .into_iter()
            .find(|a| a.address.eq_ignore_ascii_case(address));
        let account = Account {
            name: existing
                .as_ref()
                .map_or_else(|| address.to_string(), |a| a.name.clone()),
            address: address.to_string(),
            host: known.imap_host,
            port: known.imap_port,
            username: address.to_string(),
            smtp_host: known.smtp_host,
            smtp_port: known.smtp_port,
            label: None,
        };
        let backend = ImapBackend::connect(&account, Login::Bearer(&t.access))
            .await
            .map_err(backend_err)?;
        let secret = oauth::secret(cfg, &refresh);
        keyring::Entry::new(KEYRING_SERVICE, &account.address)
            .and_then(|e| e.set_password(&secret))
            .map_err(|e| backend_err(format!("keychain: {e}")))?;
        self.store.add_account(&account).map_err(backend_err)?;
        self.passwords.insert(account.address.clone(), secret);
        self.tokens.insert(account.address.clone(), t);
        self.sessions.insert(account.name.clone(), backend);
        tracing::info!(account = %account.name, "OAuth account added");
        Ok(account)
    }

    pub(crate) async fn sync(&mut self, only: Option<String>) -> Result<Vec<SyncResult>, RpcError> {
        let accounts = match only {
            Some(name) => vec![self.account(&name)?],
            None => self.store.accounts().map_err(backend_err)?,
        };
        let mut results = Vec::new();
        for account in accounts {
            self.session(&account).await?;
            let backend = self
                .sessions
                .get_mut(&account.name)
                .expect("connected above");
            let new = sync::sync_account(&mut self.store, backend, &account.name).await;
            let new = self.drop_on_err(&account.name, new)?;
            tracing::info!(account = %account.name, new, "synced");
            // Codes, bookings, orders. Never fails the sync; counts only in logs.
            let backend = self
                .sessions
                .get_mut(&account.name)
                .expect("connected above");
            match sync::extract_pending(&mut self.store, backend, &account.name).await {
                Ok(n) => tracing::info!(account = %account.name, examined = n, "extracted"),
                Err(e) => {
                    self.sessions.remove(&account.name);
                    tracing::warn!(account = %account.name, error = %e, "extraction failed");
                }
            }
            results.push(SyncResult {
                account: account.name,
                new,
            });
        }
        Ok(results)
    }

    /// Sends from one of the user's identities. A reply gets threading headers,
    /// goes to Reply-To/From, and is cleared from the Desk once sent.
    async fn send(&mut self, p: SendParams) -> Result<SendResult, RpcError> {
        let invalid = |m: &str| RpcError::new(RpcError::INVALID_PARAMS, m);
        let account = self
            .store
            .identity_account(&p.from)
            .map_err(backend_err)?
            .ok_or_else(|| invalid("`from` is not one of your addresses"))?;
        let from_name = self
            .store
            .identities()
            .map_err(backend_err)?
            .into_iter()
            .find(|i| i.address.eq_ignore_ascii_case(p.from.trim()))
            .and_then(|i| i.name);
        let mut out = Outgoing {
            from: p.from.trim().to_lowercase(),
            from_name,
            to: p.to,
            cc: p.cc,
            subject: p.subject,
            body: p.body,
            ..Default::default()
        };
        if let Some(id) = &p.reply_to {
            let ctx = self
                .store
                .reply_context(id)
                .map_err(backend_err)?
                .ok_or_else(|| RpcError::new(RpcError::NOT_FOUND, "no such message"))?;
            if out.to.is_empty() {
                out.to = vec![ctx.to];
            }
            if out.subject.is_empty() {
                out.subject = ctx.subject;
            }
            out.in_reply_to = ctx.message_id;
            out.references = ctx.references;
        }
        if out.to.is_empty() {
            return Err(invalid("no recipients"));
        }

        self.session(&account).await?;
        let backend = self
            .sessions
            .get_mut(&account.name)
            .expect("connected above");
        let sent = backend.send(&out).await;
        let message_id = self.drop_on_err(&account.name, sent)?;
        tracing::info!(account = %account.name, "sent");

        if let Some(id) = p.reply_to {
            // Already sent: a failure here must not look like a failed send.
            if let Err(e) = self.store.mark_done(&[id]) {
                tracing::warn!(error = %e, "sent, but clearing the card failed");
            }
        }
        // Pull the sent copy in, so contacts, "replied" and the From suggestion
        // learn from it. Best effort: the mail is already sent.
        if let Err(e) = self.sync(Some(account.name.clone())).await {
            tracing::warn!(account = %account.name, error = %e, "sync after send failed");
        }
        Ok(SendResult { message_id })
    }

    /// The From address for a new mail to `to`: the one you last used with them,
    /// else your first account's address.
    /// ponytail: "default account for new mail" setting arrives with the Accounts screen.
    fn suggest_identity(&self, to: &str) -> Result<IdentitySuggestion, RpcError> {
        let address = to
            .rsplit_once('<')
            .map_or(to, |(_, a)| a.trim_end_matches('>'))
            .trim();
        if let Some(identity) = self.store.last_identity_for(address).map_err(backend_err)? {
            let who = self
                .store
                .contacts(10_000)
                .map_err(backend_err)?
                .into_iter()
                .find(|c| c.address.eq_ignore_ascii_case(address))
                .and_then(|c| c.name)
                .map_or_else(
                    || address.to_string(),
                    |n| n.split_whitespace().next().unwrap_or(&n).to_string(),
                );
            return Ok(IdentitySuggestion {
                identity,
                reason: format!("You last wrote to {who} from this address"),
                usual: true,
            });
        }
        let first = self
            .store
            .accounts()
            .map_err(backend_err)?
            .into_iter()
            .next();
        Ok(IdentitySuggestion {
            identity: first.map(|a| a.address.to_lowercase()).unwrap_or_default(),
            reason: "Your default for new email".into(),
            usual: false,
        })
    }

    pub(crate) async fn show(&mut self, id: &str) -> Result<Message, RpcError> {
        let not_found = || RpcError::new(RpcError::NOT_FOUND, "no such message");
        let stored = self
            .store
            .get(id)
            .map_err(backend_err)?
            .ok_or_else(not_found)?;
        let stored = match stored.body {
            Some(_) => stored,
            None => {
                let account = self.account(&stored.summary.account)?;
                self.session(&account).await?;
                let backend = self
                    .sessions
                    .get_mut(&account.name)
                    .expect("connected above");
                let loaded = sync::load(&mut self.store, backend, id).await;
                self.drop_on_err(&account.name, loaded)?
                    .ok_or_else(not_found)?
            }
        };
        Ok(Message {
            summary: stored.summary,
            body: stored.body.unwrap_or_default(),
        })
    }

    /// Mail on the servers matching `query`, newest first (not kept).
    async fn search(&mut self, query: &str) -> Result<Vec<hysj_proto::Found>, RpcError> {
        let mut out = Vec::new();
        for account in self.store.accounts().map_err(backend_err)? {
            self.session(&account).await?;
            let backend = self
                .sessions
                .get_mut(&account.name)
                .expect("connected above");
            let found = backend.search(query, 40).await;
            for h in self.drop_on_err(&account.name, found)? {
                let parsed = mail_parser::MessageParser::default().parse_headers(&h.headers);
                let Some(m) = parsed else { continue };
                let from = m.from().and_then(|f| f.first());
                let from_addr = from
                    .and_then(|a| a.address())
                    .unwrap_or_default()
                    .to_string();
                out.push(hysj_proto::Found {
                    id: format!("found:{}|{}", account.name, h.remote_id),
                    account: account.name.clone(),
                    from: from
                        .and_then(|a| a.name())
                        .map(str::to_string)
                        .unwrap_or_else(|| from_addr.clone()),
                    from_addr,
                    subject: m.subject().unwrap_or_default().to_string(),
                    ts: m.date().map(|d| d.to_timestamp()),
                });
            }
        }
        out.sort_by_key(|f| std::cmp::Reverse(f.ts));
        Ok(out)
    }

    /// The original message, fetched from the server if it wasn't kept.
    async fn raw(&mut self, id: &str) -> Result<Vec<u8>, RpcError> {
        let not_found = || RpcError::new(RpcError::NOT_FOUND, "no such message");
        // Found by search: straight from the server, never stored.
        if let Some((account, remote)) = id.strip_prefix("found:").and_then(|r| r.split_once('|')) {
            let account = self.account(account)?;
            self.session(&account).await?;
            let backend = self
                .sessions
                .get_mut(&account.name)
                .expect("connected above");
            let raw = backend.fetch(remote).await;
            return self.drop_on_err(&account.name, raw);
        }
        if let Some(r) = self.store.raw(id).map_err(backend_err)? {
            return Ok(r);
        }
        let stored = self
            .store
            .get(id)
            .map_err(backend_err)?
            .ok_or_else(not_found)?;
        let account = self.account(&stored.summary.account)?;
        self.session(&account).await?;
        let backend = self
            .sessions
            .get_mut(&account.name)
            .expect("connected above");
        let raw = sync::raw(&mut self.store, backend, id).await;
        self.drop_on_err(&account.name, raw)?.ok_or_else(not_found)
    }

    /// One Digest delivery as an edition (latest, earlier, or everything
    /// older), with the model's headline and lede when they're written.
    pub fn edition(&self, pick: u8) -> Result<hysj_proto::Edition, hysj_core::StoreError> {
        let mut e = self.edition_at(pick, &jiff::Zoned::now())?;
        let key = hysj_core::edition::facts_key(&hysj_core::edition::facts(&e));
        if let Some((headline, lede)) = self.store.digest_text(&key)? {
            (e.headline, e.lede) = (headline, lede);
        }
        Ok(e)
    }

    /// Editions whose headline the model should write, as `(key, facts)`: the
    /// latest, and from half an hour before the next one is due, that one
    /// with the mail so far — so it's written when it's delivered.
    pub(crate) fn digest_to_write(&self, model: &str) -> Vec<(String, String)> {
        let now = jiff::Zoned::now();
        let next = hysj_core::digest::next_slot(&now);
        let mut at = vec![now.clone()];
        if next.timestamp().as_second() - now.timestamp().as_second() < 30 * 60 {
            at.push(next);
        }
        at.iter()
            .filter_map(|t| self.edition_at(0, t).ok())
            .filter(|e| !e.stories.is_empty())
            .map(|e| hysj_core::edition::facts(&e))
            .map(|f| (hysj_core::edition::facts_key(&f), f))
            .filter(|(k, _)| self.store.digest_text_pending(k, model).unwrap_or(false))
            .collect()
    }

    /// The edition as it reads at `now`, in the templates' words only.
    fn edition_at(
        &self,
        pick: u8,
        now: &jiff::Zoned,
    ) -> Result<hysj_proto::Edition, hysj_core::StoreError> {
        use hysj_core::digest;
        let now = now.clone();
        let (previous, slot) = digest::window(pick, &now);
        let (after, upto) = (
            previous.as_ref().map(|p| p.timestamp().as_second()),
            slot.timestamp().as_second(),
        );
        let msgs: Vec<_> = self
            .store
            .list(hysj_proto::Bucket::Digest, None)?
            .into_iter()
            .filter(|m| match m.ts {
                Some(t) => t <= upto && after.is_none_or(|a| t > a),
                None => pick == 0,
            })
            .collect();
        let t = now.timestamp().as_second();
        let filed = self.store.orders()?.iter().filter(|o| o.status < 4).count()
            + self
                .store
                .bookings()?
                .iter()
                .filter(|b| b.start_ts.is_some_and(|s| s >= t))
                .count();
        let next = digest::next_slot(&now);
        let ids: Vec<String> = msgs.iter().map(|m| m.id.clone()).collect();
        let listings = self.store.listings_in(&ids)?;
        // Who is a shop, judged on everything they've sent (cleared mail too).
        let mut history = self.store.list(hysj_proto::Bucket::Digest, None)?;
        history.extend(self.store.done(None, 2000)?);
        let mut shops = hysj_core::edition::shops(&history);
        shops.extend(self.store.model_shops()?);
        let photos = self.store.images_allowed("agent@finn.no")?;
        let (finn_paused, finn_drops_only) = self.store.finn_settings()?;
        let offers = self.store.offers()?;
        let mut senders = std::collections::HashSet::new();
        let unsure: Vec<hysj_proto::Unsure> = msgs
            .iter()
            .filter(|m| !matches!(m.reason, hysj_proto::Reason::Decision { .. }))
            .filter_map(|m| {
                let why = hysj_core::classify::official(&m.from_addr)?;
                let domain = m.from_addr.rsplit('@').next()?;
                let headers = self.store.headers(&m.id).ok()??;
                (hysj_core::extract::authenticated(&headers, domain)
                    && senders.insert(m.from_addr.to_lowercase()))
                .then(|| hysj_proto::Unsure {
                    id: m.id.clone(),
                    from: m.from.clone(),
                    sender: m.from_addr.clone(),
                    title: m.subject.clone(),
                    why: why.to_string(),
                })
            })
            .take(3)
            .collect();
        let mut e = hysj_core::edition::build(
            &msgs,
            &hysj_core::edition::Context {
                pick,
                slot: &slot,
                previous: previous.as_ref(),
                next: &next,
                now: &now,
                filed,
                listings: &listings,
                shops: &shops,
                photos,
                finn_paused: &finn_paused,
                finn_drops_only,
                offers: &offers,
            },
        );
        e.unsure = unsure;
        Ok(e)
    }

    pub(crate) async fn view(&mut self, id: &str) -> Result<MessageView, RpcError> {
        let raw = self.raw(id).await?;
        let v = hysj_core::view::view(&raw);
        let sender = match self.store.get(id).ok().flatten() {
            Some(m) => m.summary.from_addr,
            None => mail_parser::MessageParser::default()
                .parse_headers(&raw)
                .and_then(|m| {
                    m.from()
                        .and_then(|f| f.first())
                        .and_then(|a| a.address())
                        .map(str::to_string)
                })
                .unwrap_or_default(),
        };
        Ok(MessageView {
            id: id.to_string(),
            html: v.html,
            plain: v.plain,
            attachments: v.attachments,
            remote_images: v.remote_images,
            images_allowed: self.store.images_allowed(&sender).map_err(backend_err)?,
            sender,
        })
    }

    async fn attachment(&mut self, p: AttachmentParams) -> Result<AttachmentData, RpcError> {
        use base64::Engine;
        let raw = self.raw(&p.id).await?;
        let (name, mime, data) = hysj_core::view::attachment(&raw, p.index)
            .ok_or_else(|| RpcError::new(RpcError::NOT_FOUND, "no such attachment"))?;
        Ok(AttachmentData {
            name,
            mime,
            data: base64::engine::general_purpose::STANDARD.encode(data),
        })
    }

    /// A failed call may have left the session broken: reconnect next time.
    fn drop_on_err<T>(
        &mut self,
        account: &str,
        r: Result<T, hysj_core::BackendError>,
    ) -> Result<T, RpcError> {
        r.map_err(|e| {
            self.sessions.remove(account);
            tracing::warn!(account, error = %e, "backend call failed");
            backend_err(e)
        })
    }
}

/// Serves one client connection until it closes.
/// The daemon shared by all client connections and the scheduler.
/// ponytail: one lock; requests (and scheduled syncs) run one at a time.
pub struct Hub {
    pub daemon: tokio::sync::Mutex<Daemon>,
    events: tokio::sync::broadcast::Sender<StateChanged>,
    state: AtomicU64,
    apps: AtomicUsize,
    /// Scheduled syncs stop after a keychain error (no prompt every minute)
    /// until the user syncs by hand.
    pub sync_paused: std::sync::atomic::AtomicBool,
    /// Microsoft sign-ins waiting for the browser, by address. Apart from the
    /// daemon's lock: waiting for a person must not stop the mail.
    oauth: tokio::sync::Mutex<HashMap<String, oauth::Pending>>,
    /// The local model, when loaded (see `ai::Slot`).
    pub llm: tokio::sync::Mutex<ai::Slot>,
}

impl Hub {
    pub fn new(daemon: Daemon) -> Arc<Self> {
        Arc::new(Self {
            daemon: tokio::sync::Mutex::new(daemon),
            events: tokio::sync::broadcast::channel(16).0,
            state: AtomicU64::new(1),
            apps: AtomicUsize::new(0),
            sync_paused: std::sync::atomic::AtomicBool::new(false),
            oauth: tokio::sync::Mutex::new(HashMap::new()),
            llm: tokio::sync::Mutex::new(ai::Slot::default()),
        })
    }

    /// Syncs mail (all accounts, or one), holding the daemon's lock only to
    /// read and write the store: the waits on the mail server happen without
    /// it, so the app's requests aren't stuck behind a sync. The session is
    /// taken out of the pool meanwhile (a request needing it opens another).
    pub async fn sync(&self, only: Option<String>) -> Result<Vec<SyncResult>, RpcError> {
        use hysj_core::MailBackend;
        let accounts = {
            let d = self.daemon.lock().await;
            match only {
                Some(name) => vec![d.account(&name)?],
                None => d.store.accounts().map_err(backend_err)?,
            }
        };
        let mut results = Vec::new();
        for account in accounts {
            let name = account.name.clone();
            let (mut backend, cursor) = {
                let mut d = self.daemon.lock().await;
                d.session(&account).await?;
                let backend = d.sessions.remove(&name).expect("connected above");
                (backend, d.store.cursor(&name).map_err(backend_err)?)
            };
            let changes = backend.sync_since(cursor.as_deref()).await.map_err(|e| {
                tracing::warn!(account = %name, error = %e, "backend call failed");
                backend_err(e)
            })?;
            let (new, pending) = {
                let mut d = self.daemon.lock().await;
                let new = d.store.apply(&name, &changes).map_err(backend_err)?;
                let pending = d
                    .store
                    .pending_extracts(&name, hysj_core::sync::EXTRACT_PER_SYNC)
                    .unwrap_or_default();
                (new, pending)
            };
            tracing::info!(account = %name, new, "synced");
            // Codes, bookings, orders. Never fails the sync; counts only in logs.
            let ids: Vec<String> = pending.iter().map(|p| p.remote_id.clone()).collect();
            let bodies = if ids.is_empty() {
                Ok(Vec::new())
            } else {
                backend
                    .fetch_many(&ids, hysj_core::sync::EXTRACT_MAX_BYTES)
                    .await
            };
            let mut d = self.daemon.lock().await;
            match bodies {
                Ok(bodies) => {
                    let bodies: HashMap<String, Vec<u8>> = bodies.into_iter().collect();
                    for p in &pending {
                        // A message gone from the server is recorded as examined, not retried.
                        let found = bodies
                            .get(&p.remote_id)
                            .map(|raw| hysj_core::extract::extract(raw))
                            .unwrap_or_default();
                        if let Err(e) = d.store.record_extracts(p, &found) {
                            tracing::warn!(account = %name, error = %e, "extraction not stored");
                        }
                    }
                    tracing::info!(account = %name, examined = pending.len(), "extracted");
                    d.sessions.insert(name.clone(), backend);
                }
                // The session is suspect: dropped, reconnected next time.
                Err(e) => tracing::warn!(account = %name, error = %e, "extraction failed"),
            }
            results.push(SyncResult { account: name, new });
        }
        // Unsubscribed a week ago and still sending: blocked.
        let late = self.daemon.lock().await.store.enforce_unsubscribes();
        if let Ok(late) = late.as_deref()
            && !late.is_empty()
        {
            tracing::info!(
                senders = late.len(),
                "kept sending after unsubscribing; blocked"
            );
        }
        Ok(results)
    }

    /// Fetches the bodies these messages lack, holding the daemon's lock only
    /// around the store (as `sync` does): background work never keeps the
    /// app's requests waiting on a mail server. One that fails stays without.
    pub async fn fetch_bodies(&self, ids: &[String]) {
        use hysj_core::MailBackend;
        for id in ids {
            let (name, remote_id, mut backend) = {
                let mut d = self.daemon.lock().await;
                let Ok(Some(s)) = d.store.get(id) else {
                    continue;
                };
                if s.body.is_some() {
                    continue;
                }
                let Ok(account) = d.account(&s.summary.account) else {
                    continue;
                };
                if d.session(&account).await.is_err() {
                    continue;
                }
                let backend = d.sessions.remove(&account.name).expect("connected above");
                (account.name, s.remote_id, backend)
            };
            let raw = backend.fetch(&remote_id).await;
            let mut d = self.daemon.lock().await;
            match raw {
                Ok(raw) => {
                    let _ = sync::keep(&mut d.store, id, &raw);
                    d.sessions.insert(name, backend);
                }
                // The session is suspect: dropped, reconnected next time.
                Err(e) => tracing::warn!(account = %name, error = %e, "body fetch failed"),
            }
        }
    }

    /// Tells every connected client that mail state changed (JMAP-style: they refetch).
    pub fn changed(&self) {
        let state = self.state.fetch_add(1, Ordering::Relaxed) + 1;
        let _ = self.events.send(StateChanged {
            kind: "mail".into(),
            state,
        });
    }

    /// Whether an app window is connected (else the daemon notifies the OS itself).
    pub fn app_connected(&self) -> bool {
        self.apps.load(Ordering::Relaxed) > 0
    }
}

/// Per-connection state.
#[derive(Default)]
struct Conn {
    greeted: bool,
    app: bool,
}

/// Serves one client connection until it closes, forwarding state-changed pushes.
pub async fn serve<T: Transport>(mut transport: T, hub: Arc<Hub>) {
    let mut conn = Conn::default();
    let mut events = hub.events.subscribe();
    loop {
        let out = tokio::select! {
            raw = transport.recv() => match raw {
                Some(raw) => serde_json::to_string(&handle(&raw, &hub, &mut conn).await),
                None => break,
            },
            ev = events.recv() => match ev {
                Ok(params) => serde_json::to_string(&Notification { jsonrpc: "2.0".into(), method: method::STATE.into(), params }),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            },
        };
        if transport
            .send(out.expect("messages always serialize"))
            .await
            .is_err()
        {
            break;
        }
    }
    if conn.app {
        hub.apps.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Methods that change what clients show.
const MUTATING: &[&str] = &[
    method::SYNC,
    method::ACCOUNT_ADD,
    method::ACCOUNT_LABEL,
    method::SCREENER_DECIDE,
    method::SCREENER_FORGET,
    method::MESSAGE_DONE,
    method::MESSAGE_RESTORE,
    method::SEND,
    method::CODE_REPORT,
    method::CODE_SETTINGS,
    method::SIGNAL_HOLD,
    method::DIGEST_SET,
    method::DIGEST_HOURS,
    method::AI_SETTINGS,
    method::KIND_CORRECT,
    method::UNSUBSCRIBE,
    method::CLEANUP_KEEP,
    method::PILE_ARCHIVE,
];

async fn handle(raw: &str, hub: &Hub, conn: &mut Conn) -> Response {
    let req: Request = match serde_json::from_str(raw) {
        Ok(req) => req,
        Err(e) => return Response::err(None, RpcError::new(RpcError::PARSE, e.to_string())),
    };
    let id = req.id;
    let mutating = MUTATING.contains(&req.method.as_str());
    if req.method == method::SYNC {
        hub.sync_paused.store(false, Ordering::Relaxed);
    }
    let was_app = conn.app;
    let result = match req.method.as_str() {
        // Waits for a person in a browser: never under the daemon's lock.
        method::ACCOUNT_OAUTH_START | method::ACCOUNT_OAUTH_FINISH if conn.greeted => {
            oauth_step(req, hub).await
        }
        // Waits on mail servers: the lock only around the store (Hub::sync).
        method::SYNC if conn.greeted => match params::<SyncParams>(req.params) {
            Ok(p) => hub.sync(p.account).await.and_then(to_value),
            Err(e) => Err(e),
        },
        // Waits on the sender's server: never under the daemon's lock.
        method::UNSUBSCRIBE if conn.greeted => {
            match params::<hysj_proto::AddressParams>(req.params) {
                Ok(p) => unsubscribe(hub, &p.address).await.and_then(to_value),
                Err(e) => Err(e),
            }
        }
        // Waits on the local model: never under the daemon's lock.
        method::ASK if conn.greeted => match params::<hysj_proto::AskParams>(req.params) {
            Ok(p) => to_value(ai::ask(hub, &p.question).await),
            Err(e) => Err(e),
        },
        _ => dispatch(req, &mut *hub.daemon.lock().await, conn).await,
    };
    if conn.app && !was_app {
        hub.apps.fetch_add(1, Ordering::Relaxed);
    }
    match result {
        Ok(result) => {
            if mutating {
                hub.changed();
            }
            Response::ok(id, result)
        }
        Err(e) => Response::err(Some(id), e),
    }
}

/// Unsubscribes at the source, the best way the sender offers: RFC 8058
/// one-click (a POST, no page), else an email from the address it came to,
/// else its page for the app to open. Recorded so Clean up can check, a
/// week later, that the mail stopped.
async fn unsubscribe(hub: &Hub, address: &str) -> Result<hysj_proto::Unsubscribed, RpcError> {
    use hysj_core::cleanup::Method;
    let address = address.trim().to_lowercase();
    let method = {
        let d = hub.daemon.lock().await;
        let since = now() - hysj_core::cleanup::DAYS * 86_400;
        d.store
            .bulk_since(since)
            .map_err(backend_err)?
            .into_iter()
            .filter(|b| b.address == address)
            .max_by_key(|b| b.date)
            .map(|b| b.method)
            .unwrap_or(Method::None)
    };
    let done = |m: &str, url: Option<String>| hysj_proto::Unsubscribed {
        method: m.into(),
        url,
    };
    let result = match &method {
        Method::OneClick(url) => {
            let resp = reqwest::Client::new()
                .post(url)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body("List-Unsubscribe=One-Click")
                .timeout(std::time::Duration::from_secs(20))
                .send()
                .await
                .map_err(|e| backend_err(format!("the sender's server: {e}")))?;
            if !resp.status().is_success() {
                return Err(backend_err(format!(
                    "the sender's server answered {}",
                    resp.status()
                )));
            }
            done("one_click", None)
        }
        Method::Mailto(m) => {
            let u = reqwest::Url::parse(&format!("mailto:{m}"))
                .map_err(|_| backend_err("the sender's unsubscribe address is broken"))?;
            let subject = u
                .query_pairs()
                .find(|(k, _)| k.eq_ignore_ascii_case("subject"))
                .map_or_else(|| "unsubscribe".to_string(), |(_, v)| v.into_owned());
            let mut d = hub.daemon.lock().await;
            let from = d
                .store
                .received_on(&address)
                .map_err(backend_err)?
                .ok_or_else(|| backend_err("no mail from this sender"))?;
            d.send(SendParams {
                from,
                to: vec![u.path().to_string()],
                cc: vec![],
                subject,
                body: "unsubscribe".into(),
                reply_to: None,
            })
            .await?;
            done("mailto", None)
        }
        Method::Web(url) => done("web", Some(url.clone())),
        Method::None => {
            return Err(RpcError::new(
                RpcError::INVALID_PARAMS,
                "this sender doesn't say how to unsubscribe",
            ));
        }
    };
    hub.daemon
        .lock()
        .await
        .store
        .set_subscription(&address, Some("unsubscribed"))
        .map_err(backend_err)?;
    Ok(result)
}

/// "Sign in with …" (the address's provider): start hands out the page to
/// open; finish waits for the browser to come back, then adds the account
/// under the lock.
async fn oauth_step(req: Request, hub: &Hub) -> Result<Value, RpcError> {
    let p: hysj_proto::AccountOAuth = params(req.params)?;
    let address = p.address.trim().to_lowercase();
    let cfg = provider::oauth_for(&address).ok_or_else(|| {
        RpcError::new(
            RpcError::INVALID_PARAMS,
            "this address signs in with a password",
        )
    })?;
    let fail = |e: oauth::OAuthError| backend_err(format!("{} sign-in: {e}", cfg.name));
    if req.method == method::ACCOUNT_OAUTH_START {
        let pending = oauth::start(cfg, &address).await.map_err(fail)?;
        let url = pending.url.clone();
        hub.oauth.lock().await.insert(address, pending);
        return to_value(hysj_proto::OAuthStarted { url });
    }
    let pending = hub
        .oauth
        .lock()
        .await
        .remove(&address)
        .ok_or_else(|| RpcError::new(RpcError::INVALID_PARAMS, "start the sign-in first"))?;
    let tokens = oauth::finish(pending).await.map_err(fail)?;
    let account = hub
        .daemon
        .lock()
        .await
        .add_oauth(&address, cfg, tokens)
        .await?;
    hub.changed();
    to_value(account)
}

async fn dispatch(req: Request, daemon: &mut Daemon, conn: &mut Conn) -> Result<Value, RpcError> {
    // Method names only: never log params (addresses, passwords, bodies).
    tracing::debug!(method = %req.method, "request");
    if req.method != method::HELLO && !conn.greeted {
        return Err(RpcError::new(RpcError::HELLO_REQUIRED, "call hello first"));
    }
    match req.method.as_str() {
        method::HELLO => {
            let hello: Hello = params(req.params)?;
            if hello.version > PROTOCOL_VERSION && IS_DAEMON.load(Ordering::Relaxed) {
                // A newer client: this daemon is outdated. Answer, then make way;
                // the client starts the current build.
                tracing::info!(
                    client = hello.version,
                    "newer client; exiting so it can start the current daemon"
                );
                tokio::spawn(async {
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    std::process::exit(0);
                });
            }
            if hello.version != PROTOCOL_VERSION {
                return Err(RpcError::new(
                    RpcError::VERSION_MISMATCH,
                    format!(
                        "client speaks v{}, daemon speaks v{PROTOCOL_VERSION}",
                        hello.version
                    ),
                ));
            }
            conn.greeted = true;
            conn.app = hello.client.as_deref() == Some("app");
            to_value(HelloResult {
                version: PROTOCOL_VERSION,
                daemon: env!("CARGO_PKG_VERSION").into(),
            })
        }
        method::LIST => {
            let p: ListParams = params(req.params)?;
            to_value(
                daemon
                    .store
                    .list(p.bucket, p.account.as_deref())
                    .map_err(backend_err)?,
            )
        }
        method::SHOW => {
            let p: ShowParams = params(req.params)?;
            to_value(daemon.show(&p.id).await?)
        }
        method::MESSAGE_VIEW => {
            let p: ShowParams = params(req.params)?;
            let view = daemon.view(&p.id).await?;
            let _ = daemon.store.record_open(&p.id);
            to_value(view)
        }
        method::CLEANUP => {
            let since = now() - hysj_core::cleanup::DAYS * 86_400;
            let _ = daemon.store.enforce_unsubscribes();
            let counting_since = daemon.store.opens_since().map_err(backend_err)?;
            to_value(hysj_proto::Cleanup {
                counting_since,
                subscriptions: hysj_core::cleanup::subscriptions(
                    &daemon.store.bulk_since(since).map_err(backend_err)?,
                    &daemon.store.subscription_states().map_err(backend_err)?,
                    counting_since,
                ),
                pile: daemon
                    .store
                    .pile(now() - 365 * 86_400)
                    .map_err(backend_err)?
                    .len() as u32,
            })
        }
        method::CLEANUP_KEEP => {
            let p: hysj_proto::CleanupKeep = params(req.params)?;
            daemon
                .store
                .set_subscription(&p.address, p.keep.then_some("kept"))
                .map_err(backend_err)?;
            to_value(())
        }
        method::PILE_ARCHIVE => {
            let ids = daemon
                .store
                .pile(now() - 365 * 86_400)
                .map_err(backend_err)?;
            to_value(daemon.store.mark_done(&ids).map_err(backend_err)?)
        }
        method::MESSAGE_ATTACHMENT => {
            let p: AttachmentParams = params(req.params)?;
            to_value(daemon.attachment(p).await?)
        }
        method::SENDER_IMAGES => {
            let p: SenderImages = params(req.params)?;
            daemon
                .store
                .set_images_allowed(&p.sender, p.allow)
                .map_err(backend_err)?;
            to_value(())
        }
        method::ACCOUNT_ADD => to_value(daemon.add_account(params(req.params)?).await?),
        method::ACCOUNT_LIST => to_value(daemon.store.accounts().map_err(backend_err)?),
        method::ACCOUNT_DISCOVER => {
            let p: hysj_proto::AccountOAuth = params(req.params)?;
            to_value(provider::discover(&p.address))
        }
        method::ACCOUNT_LABEL => {
            let p: hysj_proto::AccountLabel = params(req.params)?;
            let label = p.label.as_deref().map(str::trim).filter(|l| !l.is_empty());
            if label.is_some_and(|l| l.chars().count() > 40) {
                return Err(RpcError::new(
                    RpcError::INVALID_PARAMS,
                    "keep the name under 40 characters",
                ));
            }
            let found = daemon
                .store
                .set_account_label(&p.name, label)
                .map_err(backend_err)?;
            if !found {
                return Err(RpcError::new(
                    RpcError::INVALID_PARAMS,
                    format!("no account {}", p.name),
                ));
            }
            to_value(())
        }
        method::IDENTITY_LIST => to_value(daemon.store.identities().map_err(backend_err)?),
        method::SCREENER_DECIDE => {
            let p: ScreenerDecide = params(req.params)?;
            if !p.sender.contains('@') {
                return Err(RpcError::new(
                    RpcError::INVALID_PARAMS,
                    "sender must be an email address",
                ));
            }
            let (bucket, messages) = daemon
                .store
                .decide(&p.sender, p.decision)
                .map_err(backend_err)?;
            tracing::info!(?bucket, messages, "screener decision");
            to_value(ScreenerDecided {
                sender: p.sender.trim().to_lowercase(),
                bucket,
                messages,
            })
        }
        method::SCREENER_FORGET => {
            let p: ScreenerForget = params(req.params)?;
            let messages = daemon.store.forget(&p.sender).map_err(backend_err)?;
            to_value(messages)
        }
        method::MESSAGE_DONE => {
            let p: MessageDone = params(req.params)?;
            to_value(daemon.store.mark_done(&p.ids).map_err(backend_err)?)
        }
        method::SEND => to_value(daemon.send(params(req.params)?).await?),
        method::MESSAGE_RESTORE => {
            let p: MessageDone = params(req.params)?;
            to_value(daemon.store.restore(&p.ids).map_err(backend_err)?)
        }
        method::CODE_LIST => to_value(daemon.store.codes(now()).map_err(backend_err)?),
        method::CODE_REPORT => {
            let p: CodeReport = params(req.params)?;
            daemon.store.report_code(&p.id).map_err(backend_err)?;
            to_value(())
        }
        method::CODE_SETTINGS => {
            // With params: change; always returns the current settings.
            if !req.params.is_null() {
                let s: CodeSettings = params(req.params)?;
                daemon.store.set_code_settings(&s).map_err(backend_err)?;
            }
            to_value(daemon.store.code_settings().map_err(backend_err)?)
        }
        method::AI_SETTINGS => {
            if !req.params.is_null() {
                let p: hysj_proto::AiSettings = params(req.params)?;
                daemon
                    .store
                    .set_ai_enabled(p.enabled)
                    .map_err(backend_err)?;
                daemon
                    .store
                    .set_ai_on_battery(p.on_battery)
                    .map_err(backend_err)?;
                if matches!(p.size.as_str(), "standard" | "light" | "apple") {
                    daemon.store.set_ai_size(&p.size).map_err(backend_err)?;
                }
            }
            let choice = crate::ai::chosen(&daemon.store);
            let on_battery = daemon.store.ai_on_battery().map_err(backend_err)?;
            // Asked fresh: Apple Intelligence may have been turned on or off.
            let apple = crate::ai::refresh_apple().await;
            to_value(hysj_proto::AiSettings {
                enabled: daemon.store.ai_enabled().map_err(backend_err)?,
                model: choice.version().to_string(),
                size: crate::ai::size_of(choice).into(),
                recommended: crate::ai::recommended_size().into(),
                apple_available: apple.available,
                apple_reason: apple.reason,
                on_battery,
                waiting_for_power: !on_battery && crate::ai::on_battery_power().await,
            })
        }
        method::KIND_CORRECT => {
            let p: hysj_proto::KindCorrect = params(req.params)?;
            if hysj_core::kinds::index(&p.kind).is_none() {
                return Err(RpcError::new(RpcError::INVALID_PARAMS, "unknown kind"));
            }
            daemon
                .store
                .correct_kind(&p.record, &p.kind)
                .map_err(backend_err)?;
            to_value(())
        }
        method::DIGEST_HOURS => {
            if !req.params.is_null() {
                let p: hysj_proto::DigestHours = params(req.params)?;
                if p.hours.is_empty()
                    || p.hours.len() > 5
                    || p.hours.iter().any(|h| !(0..24).contains(h))
                {
                    return Err(RpcError::new(
                        RpcError::INVALID_PARAMS,
                        "one to five whole hours, 0–23",
                    ));
                }
                daemon
                    .store
                    .set_digest_hours(&p.hours)
                    .map_err(backend_err)?;
                hysj_core::digest::set_hours(&p.hours);
            }
            to_value(hysj_core::digest::hours())
        }
        method::DIGEST_SET => {
            let p: hysj_proto::DigestSet = params(req.params)?;
            let known = p.key == "finn_drops_only"
                || p.key
                    .strip_prefix("finn_pause:")
                    .is_some_and(|k| ["home", "job", "cabin", "plot", "item"].contains(&k));
            if !known {
                return Err(RpcError::new(RpcError::INVALID_PARAMS, "unknown setting"));
            }
            daemon
                .store
                .set_digest_setting(&p.key, p.on)
                .map_err(backend_err)?;
            to_value(())
        }
        method::SIGNAL_LIST => {
            // ponytail: the last two days; "can't wait" is about now.
            let since = now() - 2 * 86_400;
            to_value(daemon.store.signals(since).map_err(backend_err)?)
        }
        method::SIGNAL_HOLD => {
            let p: hysj_proto::SignalHold = params(req.params)?;
            daemon
                .store
                .hold_signal(&p.kind, p.hold)
                .map_err(backend_err)?;
            to_value(())
        }
        method::BOOKING_LIST => to_value(daemon.store.bookings().map_err(backend_err)?),
        method::ORDER_LIST => to_value(daemon.store.orders().map_err(backend_err)?),
        method::SEARCH => {
            let p: hysj_proto::SearchParams = params(req.params)?;
            to_value(daemon.search(&p.query).await?)
        }
        method::DIGEST_EDITION => {
            let p: hysj_proto::EditionParams = params(req.params)?;
            to_value(daemon.edition(p.pick).map_err(backend_err)?)
        }
        method::FOLDER_LIST => {
            let p: FolderParams = params(req.params)?;
            let account = p.account.as_deref();
            let list = match p.folder {
                Folder::Sent => daemon.store.sent(account, 200),
                Folder::Done => daemon.store.done(account, 200),
            };
            to_value(list.map_err(backend_err)?)
        }
        method::CONTACT_LIST => to_value(daemon.store.contacts(500).map_err(backend_err)?),
        method::IDENTITY_SUGGEST => {
            let p: IdentitySuggest = params(req.params)?;
            to_value(daemon.suggest_identity(&p.to)?)
        }
        other => Err(RpcError::new(
            RpcError::METHOD_NOT_FOUND,
            format!("unknown method {other}"),
        )),
    }
}

fn params<P: DeserializeOwned>(v: Value) -> Result<P, RpcError> {
    serde_json::from_value(v).map_err(|e| RpcError::new(RpcError::INVALID_PARAMS, e.to_string()))
}

fn to_value(v: impl serde::Serialize) -> Result<Value, RpcError> {
    Ok(serde_json::to_value(v).expect("result types always serialize"))
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn backend_err(e: impl Display) -> RpcError {
    RpcError::new(RpcError::BACKEND, e.to_string())
}
