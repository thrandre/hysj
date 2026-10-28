use std::path::Path;

use hysj_proto::{Account, Bucket, Contact, Decision, Identity, Intent, MessageSummary};
use mail_parser::MessageParser;
use rusqlite::{Connection, OptionalExtension, params};

use crate::backend::{Changes, SENT_TAG};
use crate::classify::{self, Context};

pub use rusqlite::Error as StoreError;

/// Schema v1. Later versions are applied as steps in [`MIGRATIONS`].
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS accounts (
    name TEXT PRIMARY KEY,
    address TEXT NOT NULL,
    host TEXT NOT NULL,
    port INTEGER NOT NULL,
    username TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY,
    account TEXT NOT NULL REFERENCES accounts(name) ON DELETE CASCADE,
    remote_id TEXT NOT NULL,
    message_id TEXT,
    from_name TEXT,
    from_addr TEXT NOT NULL,
    subject TEXT NOT NULL,
    date INTEGER,
    bucket TEXT NOT NULL,
    reason TEXT NOT NULL,
    headers BLOB NOT NULL,
    UNIQUE (account, remote_id)
);
CREATE INDEX IF NOT EXISTS messages_bucket_date ON messages (bucket, date DESC);
CREATE TABLE IF NOT EXISTS bodies (
    message INTEGER PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    text TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS sync_state (
    account TEXT PRIMARY KEY REFERENCES accounts(name) ON DELETE CASCADE,
    cursor TEXT NOT NULL
);
";

/// `MIGRATIONS[i]` takes the schema from version `i + 1` to `i + 2`.
const MIGRATIONS: &[&str] = &[
    // v2: tags, provider ids, identities, dedupe index.
    "ALTER TABLE messages ADD COLUMN tags TEXT NOT NULL DEFAULT '[]';
     ALTER TABLE messages ADD COLUMN provider_id TEXT;
     CREATE INDEX messages_message_id ON messages (message_id);
     CREATE TABLE identities (
         account TEXT NOT NULL REFERENCES accounts(name) ON DELETE CASCADE,
         address TEXT NOT NULL,  -- lower case
         name TEXT,
         PRIMARY KEY (account, address)
     );
     INSERT INTO identities (account, address) SELECT name, lower(address) FROM accounts;",
    // v3: rules tier. Reasons become JSON (`hysj_proto::Reason`); rows are
    // reclassified right after this migration.
    "CREATE TABLE sent_recipients (
         account TEXT NOT NULL REFERENCES accounts(name) ON DELETE CASCADE,
         remote_id TEXT NOT NULL,  -- the sent message
         address TEXT NOT NULL,    -- lower case
         name TEXT,
         PRIMARY KEY (account, remote_id, address)
     );
     CREATE TABLE sender_decisions (
         address TEXT PRIMARY KEY,  -- lower case; global across accounts
         decision TEXT NOT NULL,
         decided_at INTEGER NOT NULL
     );",
    // v4: key/value metadata (rules version).
    "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
    // v5: Desk. `done` is keyed by Message-ID (else account:remote_id) so it
    // survives resyncs and covers every account's copy. Clearing sync_state
    // re-reads the sent history, now with In-Reply-To/References.
    "CREATE TABLE done (key TEXT PRIMARY KEY, at INTEGER NOT NULL);
     CREATE TABLE replied (
         account TEXT NOT NULL REFERENCES accounts(name) ON DELETE CASCADE,
         remote_id TEXT NOT NULL,   -- the sent message
         message_id TEXT NOT NULL,  -- what it answers
         PRIMARY KEY (account, remote_id, message_id)
     );
     ALTER TABLE messages ADD COLUMN intent TEXT NOT NULL DEFAULT 'read';
     ALTER TABLE messages ADD COLUMN replied INTEGER NOT NULL DEFAULT 0;
     DELETE FROM sync_state;",
    // v6: sending. SMTP settings, the address mail arrived on, and which of
    // your addresses sent each sent message (and when) for Compose's From
    // suggestion. Clearing sync_state re-reads the sent history with From/Date.
    "ALTER TABLE accounts ADD COLUMN smtp_host TEXT NOT NULL DEFAULT '';
     ALTER TABLE accounts ADD COLUMN smtp_port INTEGER NOT NULL DEFAULT 465;
     UPDATE accounts SET smtp_host = CASE WHEN host = 'imap.gmail.com' THEN 'smtp.gmail.com'
                                         ELSE replace(host, 'imap.', 'smtp.') END;
     ALTER TABLE messages ADD COLUMN arrived_on TEXT NOT NULL DEFAULT '';
     ALTER TABLE sent_recipients ADD COLUMN identity TEXT;
     ALTER TABLE sent_recipients ADD COLUMN sent_at INTEGER;
     DELETE FROM sync_state;",
    // v7: v6's sent-history fetch left out the Date header; read it once more.
    "DELETE FROM sync_state;",
    // v8: Codes, Bookings, Orders. Keys are message keys (like `done`) so
    // resyncs don't re-download bodies; bookings/orders merge across mails.
    "CREATE TABLE extracted (key TEXT PRIMARY KEY, kind TEXT, at INTEGER NOT NULL);
     CREATE TABLE codes (
         key TEXT PRIMARY KEY,
         kind TEXT NOT NULL,
         service TEXT NOT NULL,
         sender_domain TEXT NOT NULL,
         code TEXT,                 -- whole while live, then the last two digits
         link TEXT,
         link_host TEXT,
         link_ok INTEGER NOT NULL DEFAULT 0,
         arrived INTEGER NOT NULL,
         expires INTEGER NOT NULL,
         reported INTEGER NOT NULL DEFAULT 0
     );
     CREATE TABLE bookings (
         key TEXT PRIMARY KEY, kind TEXT NOT NULL, reference TEXT NOT NULL, title TEXT NOT NULL,
         start TEXT, end_at TEXT, start_ts INTEGER, provider TEXT, city TEXT, seat TEXT,
         party INTEGER, moved_from TEXT, updated INTEGER NOT NULL
     );
     CREATE TABLE booking_sources (booking TEXT NOT NULL, message TEXT NOT NULL, date INTEGER, PRIMARY KEY (booking, message));
     CREATE TABLE orders (
         key TEXT PRIMARY KEY, store TEXT NOT NULL, number TEXT NOT NULL, ordered TEXT,
         ordered_ts INTEGER, items TEXT NOT NULL DEFAULT '[]', total TEXT, status INTEGER NOT NULL,
         eta TEXT, delivered_ts INTEGER, tracking_url TEXT, return_days INTEGER, receipt TEXT
     );
     CREATE TABLE order_sources (order_key TEXT NOT NULL, message TEXT NOT NULL, PRIMARY KEY (order_key, message));",
    // v9: results of the local model, keyed like `done` so resyncs don't redo
    // inference. Failures and rejected answers are recorded too (ok = 0) so a
    // bad message isn't retried forever.
    "CREATE TABLE ai_cache (
         key TEXT NOT NULL, model TEXT NOT NULL, task TEXT NOT NULL,
         result TEXT, ok INTEGER NOT NULL, attempts INTEGER NOT NULL, at INTEGER NOT NULL,
         PRIMARY KEY (key, model, task)
     );",
    // v10: the original message, so the app can render HTML and attachments
    // (never for code mail, capped in size); senders whose remote images load.
    "ALTER TABLE bodies ADD COLUMN raw BLOB;
     CREATE TABLE image_senders (address TEXT PRIMARY KEY);",
    // v11: listings read from alert mail (FINN), one row per ad across alerts;
    // `prev_price` remembers the price before the latest change.
    "CREATE TABLE listings (
         key TEXT PRIMARY KEY, message INTEGER NOT NULL, kind TEXT NOT NULL, title TEXT NOT NULL,
         price TEXT NOT NULL, place TEXT NOT NULL, detail TEXT NOT NULL, link TEXT NOT NULL,
         image TEXT, first_seen INTEGER NOT NULL, prev_price TEXT
     );
     CREATE INDEX listings_message ON listings (message);",
    // v12: FINN alerts read again (houses "på stor tomt" were filed as plots).
    "DELETE FROM extracted WHERE key IN (
         SELECT coalesce(message_id, account || ':' || remote_id) FROM messages
         WHERE lower(from_addr) LIKE '%@finn.no')",
    // v13: the model's first bookings were mostly orders and ads: read again.
    "DELETE FROM bookings WHERE key IN (
         SELECT s.booking FROM booking_sources s JOIN ai_cache a
         ON a.key = s.message AND a.task = 'booking' AND a.result = 'booking');
     DELETE FROM booking_sources WHERE booking NOT IN (SELECT key FROM bookings);
     DELETE FROM ai_cache WHERE task = 'booking'",
    // v14: again, now that purchases become orders and receipts aren't bookings.
    "DELETE FROM bookings WHERE key IN (
         SELECT s.booking FROM booking_sources s JOIN ai_cache a
         ON a.key = s.message AND a.task = 'booking' AND a.result = 'booking');
     DELETE FROM booking_sources WHERE booking NOT IN (SELECT key FROM bookings);
     DELETE FROM ai_cache WHERE task = 'booking'",
    // v15: read again with sender history, linking by number and no sampling noise.
    "DELETE FROM bookings WHERE key IN (
         SELECT s.booking FROM booking_sources s JOIN ai_cache a
         ON a.key = s.message AND a.task = 'booking' AND a.result = 'booking');
     DELETE FROM orders WHERE key IN (
         SELECT s.order_key FROM order_sources s JOIN ai_cache a
         ON a.key = s.message AND a.task = 'booking' AND a.result LIKE 'order:%');
     DELETE FROM order_sources WHERE order_key NOT IN (SELECT key FROM orders);
     DELETE FROM booking_sources WHERE booking NOT IN (SELECT key FROM bookings);
     DELETE FROM ai_cache WHERE task = 'booking'",
    // v16: FINN alerts read again (flats without "Totalpris" were items,
    // footer text was a fact); Digest headlines rewritten under the new check.
    "DELETE FROM extracted WHERE key IN (
         SELECT coalesce(message_id, account || ':' || remote_id) FROM messages
         WHERE lower(from_addr) LIKE '%@finn.no');
     DELETE FROM ai_cache WHERE task = 'digest'",
    // v17: newsletter gists written again as teasers (and offers spotted).
    "DELETE FROM ai_cache WHERE task = 'gist' AND key IN (
         SELECT coalesce(message_id, account || ':' || remote_id) FROM messages
         WHERE bucket = 'digest' AND reason LIKE '%\"bulk\"%' AND reason NOT LIKE '%Auto-Submitted%')",
    // v18: what the reader calls each account.
    "ALTER TABLE accounts ADD COLUMN label TEXT",
    // v19: model bookings read again: a time to show up must be one the
    // mail states (a subscription's start date was a booking).
    "DELETE FROM bookings WHERE key IN (
         SELECT s.booking FROM booking_sources s JOIN ai_cache a
         ON a.key = s.message AND a.task = 'booking' AND a.result = 'booking');
     DELETE FROM booking_sources WHERE booking NOT IN (SELECT key FROM bookings);
     DELETE FROM ai_cache WHERE task = 'booking' AND result = 'booking'",
    // v20: a classifier decides what kind of mail it is (kinds.rs): sentence
    // vectors, the reader's corrections, and everything the model filed read again.
    "CREATE TABLE vectors (key TEXT NOT NULL, model TEXT NOT NULL, vec BLOB NOT NULL, PRIMARY KEY (key, model));
     CREATE TABLE kind_labels (key TEXT PRIMARY KEY, kind TEXT NOT NULL, at INTEGER NOT NULL);
     DELETE FROM bookings WHERE key IN (
         SELECT s.booking FROM booking_sources s JOIN ai_cache a
         ON a.key = s.message AND a.task = 'booking' AND a.ok = 1);
     DELETE FROM orders WHERE key IN (
         SELECT s.order_key FROM order_sources s JOIN ai_cache a
         ON a.key = s.message AND a.task = 'booking' AND a.result LIKE 'order:%');
     DELETE FROM order_sources WHERE order_key NOT IN (SELECT key FROM orders);
     DELETE FROM booking_sources WHERE booking NOT IN (SELECT key FROM bookings);
     DELETE FROM ai_cache WHERE task = 'booking'",
];

/// SQL for a message's key in the `done` table.
const DONE_KEY: &str =
    "coalesce(messages.message_id, messages.account || ':' || messages.remote_id)";

/// The SQLite store. Only the daemon opens it.
pub struct Store {
    db: Connection,
}

pub struct ReplyContext {
    pub account: String,
    /// Your address the message was sent to; the reply goes out from it.
    pub arrived_on: String,
    /// Who to reply to (Reply-To, else From), `Name <address>`.
    pub to: String,
    pub subject: String,
    pub message_id: Option<String>,
    /// References for the reply: the original's, plus its Message-ID.
    pub references: Vec<String>,
}

/// A stored message plus what the daemon needs to load its body lazily.
pub struct Stored {
    pub summary: MessageSummary,
    pub remote_id: String,
    pub body: Option<String>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(db: Connection) -> Result<Self, StoreError> {
        db.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
        // Version 0 = fresh file, or a v1 store from before versioning (SCHEMA is idempotent).
        let mut version = db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? as usize;
        if version == 0 {
            db.execute_batch(SCHEMA)?;
            version = 1;
        }
        for (i, step) in MIGRATIONS.iter().enumerate().skip(version - 1) {
            db.execute_batch(&format!(
                "BEGIN; {step}; PRAGMA user_version = {}; COMMIT;",
                i + 2
            ))?;
        }
        // Codes the code rules no longer find (they once matched part of a
        // word: "CodePen"): forgotten, their mail examined and sorted as mail.
        let code_rules: Option<String> = db
            .query_row("SELECT value FROM meta WHERE key = 'code_rules'", [], |r| {
                r.get(0)
            })
            .optional()?;
        if code_rules != Some(crate::extract::CODE_RULES.to_string()) {
            let mut q = db.prepare(&format!(
                "SELECT DISTINCT c.key, messages.subject FROM codes c JOIN messages ON {DONE_KEY} = c.key"
            ))?;
            let stale: Vec<String> = q
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .filter(|(_, s)| {
                    crate::extract::candidate(s) != Some(crate::extract::Candidate::Code)
                })
                .map(|(k, _)| k)
                .collect();
            drop(q);
            for key in &stale {
                db.execute("DELETE FROM codes WHERE key = ?1", [key])?;
                db.execute("UPDATE extracted SET kind = NULL WHERE key = ?1", [key])?;
            }
            if !stale.is_empty() {
                reclassify(&db)?;
            }
            db.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('code_rules', ?1)",
                [crate::extract::CODE_RULES.to_string()],
            )?;
        }
        // Rules changed since this store was last sorted (or it never was): re-sort.
        let rules: Option<String> = db
            .query_row(
                "SELECT value FROM meta WHERE key = 'rules_version'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if rules != Some(classify::RULES_VERSION.to_string()) {
            reclassify(&db)?;
            db.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('rules_version', ?1)",
                [classify::RULES_VERSION.to_string()],
            )?;
        }
        Ok(Self { db })
    }

    /// Held mail since `since` that can't wait for the digest (see
    /// `classify::signal`): authenticated senders only, not done, kinds the
    /// reader hasn't held; one entry per kind and sender, newest first.
    pub fn signals(&self, since: i64) -> Result<Vec<hysj_proto::Signal>, StoreError> {
        let held: Vec<String> = {
            let mut q = self
                .db
                .prepare("SELECT substr(key, 13) FROM meta WHERE key LIKE 'held_signal:%'")?;
            q.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?
        };
        let mut q = self.db.prepare(&format!(
            "SELECT id, coalesce(from_name, from_addr), from_addr, subject, date, headers FROM messages
             WHERE bucket = 'digest' AND date > ?1 AND {DONE_KEY} NOT IN (SELECT key FROM done)
             ORDER BY date DESC"
        ))?;
        type Row = (i64, String, String, String, Option<i64>, Vec<u8>);
        let rows: Vec<Row> = q
            .query_map([since], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })?
            .collect::<Result<_, _>>()?;
        let mut out: Vec<(String, hysj_proto::Signal)> = Vec::new();
        for (id, from, addr, subject, date, headers) in rows {
            let Some(kind) = crate::classify::signal(&subject, &headers) else {
                continue;
            };
            let domain = addr.rsplit('@').next().unwrap_or_default();
            if held.iter().any(|h| h == kind) || !crate::extract::authenticated(&headers, domain) {
                continue;
            }
            let group = format!("{kind}:{}", addr.to_lowercase());
            match out.iter_mut().find(|(g, _)| *g == group) {
                Some((_, s)) => s.ids.push(id.to_string()),
                None => out.push((
                    group,
                    hysj_proto::Signal {
                        id: id.to_string(),
                        ids: vec![id.to_string()],
                        kind: kind.into(),
                        from,
                        title: subject,
                        ts: date,
                    },
                )),
            }
        }
        Ok(out.into_iter().map(|(_, s)| s).collect())
    }

    /// Newsletter issues the model found to be mainly selling (message ids):
    /// the Digest files them as promotions.
    pub fn offers(&self) -> Result<std::collections::HashSet<String>, StoreError> {
        let mut q = self.db.prepare(&format!(
            "SELECT messages.id FROM messages JOIN ai_cache a ON a.key = {DONE_KEY}
             WHERE a.task = 'offer' AND a.ok = 1"
        ))?;
        let ids = q
            .query_map([], |r| r.get::<_, i64>(0))?
            .map(|r| r.map(|i| i.to_string()))
            .collect::<Result<_, _>>()?;
        Ok(ids)
    }

    /// Whether the reader lets the on-device AI run (on unless turned off).
    pub fn ai_enabled(&self) -> Result<bool, StoreError> {
        let off: bool = self.db.query_row(
            "SELECT EXISTS (SELECT 1 FROM meta WHERE key = 'ai_off')",
            [],
            |r| r.get(0),
        )?;
        Ok(!off)
    }

    pub fn set_ai_enabled(&self, on: bool) -> Result<(), StoreError> {
        self.set_digest_setting("ai_off", !on)
    }

    /// The model size the reader chose ("standard" or "light"), if they did.
    pub fn ai_size(&self) -> Result<Option<String>, StoreError> {
        self.db
            .query_row("SELECT value FROM meta WHERE key = 'ai_size'", [], |r| {
                r.get(0)
            })
            .optional()
    }

    pub fn set_ai_size(&self, size: &str) -> Result<(), StoreError> {
        self.db.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('ai_size', ?1)",
            [size],
        )?;
        Ok(())
    }

    /// Whether the AI may run on battery (off unless the reader allows it).
    pub fn ai_on_battery(&self) -> Result<bool, StoreError> {
        self.db.query_row(
            "SELECT EXISTS (SELECT 1 FROM meta WHERE key = 'ai_on_battery')",
            [],
            |r| r.get(0),
        )
    }

    pub fn set_ai_on_battery(&self, on: bool) -> Result<(), StoreError> {
        self.set_digest_setting("ai_on_battery", on)
    }

    /// The reader's digest hours (empty: never chosen, the default applies).
    pub fn digest_hours(&self) -> Result<Vec<i8>, StoreError> {
        let v: Option<String> = self
            .db
            .query_row(
                "SELECT value FROM meta WHERE key = 'digest_hours'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        Ok(v.unwrap_or_default()
            .split(',')
            .filter_map(|h| h.trim().parse().ok())
            .collect())
    }

    pub fn set_digest_hours(&self, hours: &[i8]) -> Result<(), StoreError> {
        let v = hours
            .iter()
            .map(i8::to_string)
            .collect::<Vec<_>>()
            .join(",");
        self.db.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('digest_hours', ?1)",
            [v],
        )?;
        Ok(())
    }

    /// The reader's FINN settings: paused kinds ("cabin") and price drops only.
    pub fn finn_settings(&self) -> Result<(Vec<String>, bool), StoreError> {
        let mut q = self
            .db
            .prepare("SELECT substr(key, 12) FROM meta WHERE key LIKE 'finn_pause:%'")?;
        let paused = q.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
        let drops: bool = self.db.query_row(
            "SELECT EXISTS (SELECT 1 FROM meta WHERE key = 'finn_drops_only')",
            [],
            |r| r.get(0),
        )?;
        Ok((paused, drops))
    }

    /// Turns a Digest setting on or off: `finn_pause:<kind>` or `finn_drops_only`.
    pub fn set_digest_setting(&self, key: &str, on: bool) -> Result<(), StoreError> {
        if on {
            self.db.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, '1')",
                [key],
            )?;
        } else {
            self.db.execute("DELETE FROM meta WHERE key = ?1", [key])?;
        }
        Ok(())
    }

    /// Holds one kind of signal for the digest (or lets it through again).
    pub fn hold_signal(&self, kind: &str, hold: bool) -> Result<(), StoreError> {
        let key = format!("held_signal:{kind}");
        if hold {
            self.db.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, '1')",
                [&key],
            )?;
        } else {
            self.db.execute("DELETE FROM meta WHERE key = ?1", [&key])?;
        }
        Ok(())
    }

    /// A message's stored headers (for checks such as DMARC alignment).
    pub fn headers(&self, id: &str) -> Result<Option<Vec<u8>>, StoreError> {
        self.db
            .query_row("SELECT headers FROM messages WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()
    }

    /// Adds an account, or changes an existing one's servers (its mail stays).
    pub fn add_account(&self, a: &Account) -> Result<(), StoreError> {
        self.db.execute(
            "INSERT INTO accounts (name, address, host, port, username, smtp_host, smtp_port)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT (name) DO UPDATE SET host = excluded.host, port = excluded.port,
                 username = excluded.username, smtp_host = excluded.smtp_host, smtp_port = excluded.smtp_port",
            params![
                a.name,
                a.address,
                a.host,
                a.port,
                a.username,
                a.smtp_host,
                a.smtp_port
            ],
        )?;
        self_identity(&self.db, &a.name, &a.address, None)
    }

    /// Send-from addresses: each account's own address plus any seen on mail it sent.
    pub fn identities(&self) -> Result<Vec<Identity>, StoreError> {
        let mut q = self
            .db
            .prepare("SELECT account, address, name FROM identities ORDER BY account, address")?;
        q.query_map([], |r| {
            Ok(Identity {
                account: r.get(0)?,
                address: r.get(1)?,
                name: r.get(2)?,
            })
        })?
        .collect()
    }

    pub fn accounts(&self) -> Result<Vec<Account>, StoreError> {
        let mut q = self
            .db
            .prepare("SELECT name, address, host, port, username, smtp_host, smtp_port, label FROM accounts ORDER BY name")?;
        q.query_map([], |r| {
            Ok(Account {
                name: r.get(0)?,
                address: r.get(1)?,
                host: r.get(2)?,
                port: r.get(3)?,
                username: r.get(4)?,
                smtp_host: r.get(5)?,
                smtp_port: r.get(6)?,
                label: r.get(7)?,
            })
        })?
        .collect()
    }

    /// Names an account as the reader likes ("Work"); `None` clears it.
    pub fn set_account_label(&self, name: &str, label: Option<&str>) -> Result<bool, StoreError> {
        Ok(self.db.execute(
            "UPDATE accounts SET label = ?2 WHERE name = ?1",
            params![name, label],
        )? > 0)
    }

    pub fn cursor(&self, account: &str) -> Result<Option<String>, StoreError> {
        self.db
            .query_row(
                "SELECT cursor FROM sync_state WHERE account = ?1",
                [account],
                |r| r.get(0),
            )
            .optional()
    }

    /// Applies one sync batch atomically. Returns how many messages were new.
    pub fn apply(&mut self, account: &str, changes: &Changes) -> Result<usize, StoreError> {
        let tx = self.db.transaction()?;
        if changes.reset {
            tx.execute("DELETE FROM messages WHERE account = ?1", [account])?;
        }
        // Removals before inserts, so a message that arrived mid-sync is never
        // mistaken for a deleted one.
        if let Some(present) = &changes.present {
            let present: std::collections::HashSet<&str> =
                present.iter().map(String::as_str).collect();
            let stored: Vec<String> = tx
                .prepare("SELECT remote_id FROM messages WHERE account = ?1")?
                .query_map([account], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            for gone in stored.iter().filter(|r| !present.contains(r.as_str())) {
                tx.execute(
                    "DELETE FROM messages WHERE account = ?1 AND remote_id = ?2",
                    [account, gone],
                )?;
            }
        }
        if changes.sent_reset {
            tx.execute("DELETE FROM sent_recipients WHERE account = ?1", [account])?;
            tx.execute("DELETE FROM replied WHERE account = ?1", [account])?;
        }
        for raw in &changes.sent {
            let msg = MessageParser::default().parse_headers(&raw.headers);
            let from = msg.as_ref().and_then(|m| m.from()).and_then(|f| f.first());
            let identity = from.and_then(|f| f.address()).map(str::to_lowercase);
            if let (Some(from), Some(address)) = (from, &identity) {
                self_identity(&tx, account, address, from.name())?;
            }
            let sent_at = msg
                .as_ref()
                .and_then(|m| m.date())
                .map(|d| d.to_timestamp());
            for (address, name) in classify::recipients(&raw.headers) {
                tx.execute(
                    "INSERT OR IGNORE INTO sent_recipients (account, remote_id, address, name, identity, sent_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![account, raw.remote_id, address, name, identity, sent_at],
                )?;
            }
            for message_id in classify::answered(&raw.headers) {
                tx.execute(
                    "INSERT OR IGNORE INTO replied (account, remote_id, message_id) VALUES (?1, ?2, ?3)",
                    params![account, raw.remote_id, message_id],
                )?;
            }
        }
        for (remote_id, tags) in &changes.retagged {
            tx.execute(
                "UPDATE messages SET tags = ?3 WHERE account = ?1 AND remote_id = ?2",
                params![account, remote_id, tags_json(tags)],
            )?;
        }
        let mut added = 0;
        for raw in &changes.new {
            let Some(msg) = MessageParser::default().parse_headers(&raw.headers) else {
                continue;
            };
            let from = msg.from().and_then(|f| f.first());
            let from_addr = from.and_then(|a| a.address()).unwrap_or_default();
            if raw.tags.iter().any(|t| t == SENT_TAG) && !from_addr.is_empty() {
                self_identity(&tx, account, from_addr, from.and_then(|a| a.name()))?;
            }
            // Bucket and reason are filled in by `reclassify` below.
            added += tx.execute(
                "INSERT OR IGNORE INTO messages
                 (account, remote_id, message_id, from_name, from_addr, subject, date, bucket, reason, headers, tags, provider_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'screener', '{\"kind\":\"first_time\"}', ?8, ?9, ?10)",
                params![
                    account,
                    raw.remote_id,
                    msg.message_id(),
                    from.and_then(|a| a.name()),
                    from_addr,
                    msg.subject().unwrap_or_default(),
                    msg.date().map(|d| d.to_timestamp()),
                    raw.headers,
                    tags_json(&raw.tags),
                    raw.provider_id,
                ],
            )?;
        }
        tx.execute(
            "INSERT INTO sync_state (account, cursor) VALUES (?1, ?2)
             ON CONFLICT (account) DO UPDATE SET cursor = excluded.cursor",
            params![account, changes.cursor],
        )?;
        reclassify(&tx)?;
        tx.commit()?;
        Ok(added)
    }

    /// Records a Screener decision (global) and re-sorts. Returns the bucket the
    /// sender's mail now lives in and how many messages that is.
    pub fn decide(
        &mut self,
        sender: &str,
        decision: Decision,
    ) -> Result<(Bucket, usize), StoreError> {
        let sender = sender.trim().to_lowercase();
        let tx = self.db.transaction()?;
        tx.execute(
            "INSERT INTO sender_decisions (address, decision, decided_at) VALUES (?1, ?2, unixepoch())
             ON CONFLICT (address) DO UPDATE SET decision = excluded.decision, decided_at = excluded.decided_at",
            params![sender, decision_str(decision)],
        )?;
        reclassify(&tx)?;
        let n: i64 = tx.query_row(
            "SELECT count(*) FROM messages WHERE lower(from_addr) = ?1",
            [&sender],
            |r| r.get(0),
        )?;
        tx.commit()?;
        let bucket = match decision {
            Decision::Allow => Bucket::People,
            Decision::Digest => Bucket::Digest,
            Decision::Block => Bucket::Blocked,
        };
        Ok((bucket, n as usize))
    }

    /// Newest first. Leaves out mail the user sent. Without an account filter,
    /// a message delivered to several accounts (same Message-ID) appears once.
    pub fn list(
        &self,
        bucket: Bucket,
        account: Option<&str>,
    ) -> Result<Vec<MessageSummary>, StoreError> {
        let mut q = self.db.prepare(&format!(
            "SELECT {SUMMARY_COLS} FROM messages
             WHERE bucket = ?1 AND (?2 IS NULL OR account = ?2)
               AND lower(from_addr) NOT IN (SELECT address FROM identities)
               AND {DONE_KEY} NOT IN (SELECT key FROM done)
               AND (?2 IS NOT NULL OR message_id IS NULL
                    OR id = (SELECT min(id) FROM messages m2 WHERE m2.message_id = messages.message_id))
             ORDER BY date DESC"
        ))?;
        q.query_map(params![bucket_str(bucket), account], summary_row)?
            .collect()
    }

    pub fn get(&self, id: &str) -> Result<Option<Stored>, StoreError> {
        self.db
            .query_row(
                &format!(
                    "SELECT {SUMMARY_COLS}, remote_id, text FROM messages
                     LEFT JOIN bodies ON bodies.message = messages.id WHERE id = ?1"
                ),
                [id],
                |r| {
                    Ok(Stored {
                        summary: summary_row(r)?,
                        remote_id: r.get(14)?,
                        body: r.get(15)?,
                    })
                },
            )
            .optional()
    }

    /// Every kept mail with its text, for Ask your mail (blocked mail left out).
    pub fn searchable(&self) -> Result<Vec<crate::ask::Doc>, StoreError> {
        let mut q = self.db.prepare(
            "SELECT id, coalesce(from_name, from_addr), subject, date, text FROM messages
             JOIN bodies ON bodies.message = messages.id WHERE bucket != 'blocked'",
        )?;
        let rows = q.query_map([], |r| {
            Ok(crate::ask::Doc {
                id: r.get::<_, i64>(0)?.to_string(),
                from: r.get(1)?,
                subject: r.get(2)?,
                ts: r.get(3)?,
                body: r.get(4)?,
            })
        })?;
        rows.collect()
    }

    /// Mail you sent, newest first, with recipients filled in.
    /// ponytail: only what's in the synced mailbox (Gmail All Mail has it; generic
    /// IMAP syncs INBOX, so its Sent shows little until Sent is synced too).
    pub fn sent(
        &self,
        account: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MessageSummary>, StoreError> {
        let mut q = self.db.prepare(&format!(
            "SELECT {SUMMARY_COLS}, headers FROM messages
             WHERE lower(from_addr) IN (SELECT address FROM identities)
               AND (?1 IS NULL OR account = ?1)
               AND (?1 IS NOT NULL OR message_id IS NULL
                    OR id = (SELECT min(id) FROM messages m2 WHERE m2.message_id = messages.message_id))
             ORDER BY date DESC LIMIT ?2"
        ))?;
        q.query_map(params![account, limit as i64], |r| {
            let mut m = summary_row(r)?;
            let headers: Vec<u8> = r.get(14)?;
            let to: Vec<String> = classify::recipients(&headers)
                .into_iter()
                .map(|(a, n)| n.unwrap_or(a))
                .collect();
            m.to = Some(to.join(", "));
            Ok(m)
        })?
        .collect()
    }

    /// Mail you cleared, most recently cleared first.
    pub fn done(
        &self,
        account: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MessageSummary>, StoreError> {
        let mut q = self.db.prepare(&format!(
            "SELECT {SUMMARY_COLS} FROM messages JOIN done ON done.key = {DONE_KEY}
             WHERE (?1 IS NULL OR account = ?1)
               AND (?1 IS NOT NULL OR message_id IS NULL
                    OR id = (SELECT min(id) FROM messages m2 WHERE m2.message_id = messages.message_id))
             ORDER BY done.at DESC, date DESC LIMIT ?2"
        ))?;
        q.query_map(params![account, limit as i64], summary_row)?
            .collect()
    }

    /// Puts cleared messages back (every account's copy).
    pub fn restore(&mut self, ids: &[String]) -> Result<usize, StoreError> {
        let tx = self.db.transaction()?;
        let mut n = 0;
        for id in ids {
            n += tx.execute(
                &format!(
                    "DELETE FROM done WHERE key IN (SELECT {DONE_KEY} FROM messages WHERE id = ?1)"
                ),
                [id],
            )?;
        }
        tx.commit()?;
        Ok(n)
    }

    /// Clears messages from the Desk / Digest (all accounts' copies).
    /// ponytail: local only; archive on the server once the backend can apply changes.
    pub fn mark_done(&mut self, ids: &[String]) -> Result<usize, StoreError> {
        let tx = self.db.transaction()?;
        let mut n = 0;
        for id in ids {
            n += tx.execute(
                &format!("INSERT OR IGNORE INTO done (key, at) SELECT {DONE_KEY}, unixepoch() FROM messages WHERE id = ?1"),
                [id],
            )?;
        }
        tx.commit()?;
        Ok(n)
    }

    /// Undoes a Screener decision. Returns how many of the sender's messages there are.
    pub fn forget(&mut self, sender: &str) -> Result<usize, StoreError> {
        let sender = sender.trim().to_lowercase();
        let tx = self.db.transaction()?;
        tx.execute("DELETE FROM sender_decisions WHERE address = ?1", [&sender])?;
        reclassify(&tx)?;
        let n: i64 = tx.query_row(
            "SELECT count(*) FROM messages WHERE lower(from_addr) = ?1",
            [&sender],
            |r| r.get(0),
        )?;
        tx.commit()?;
        Ok(n as usize)
    }

    /// The account an identity (send-from address) belongs to.
    pub fn identity_account(&self, address: &str) -> Result<Option<Account>, StoreError> {
        let name: Option<String> = self
            .db
            .query_row(
                "SELECT account FROM identities WHERE address = lower(?1)",
                [address.trim()],
                |r| r.get(0),
            )
            .optional()?;
        Ok(name.and_then(|n| self.accounts().ok()?.into_iter().find(|a| a.name == n)))
    }

    /// The address you most recently wrote to `to` from, if you ever did.
    pub fn last_identity_for(&self, to: &str) -> Result<Option<String>, StoreError> {
        self.db
            .query_row(
                "SELECT identity FROM sent_recipients
                 WHERE address = lower(?1) AND identity IS NOT NULL
                 ORDER BY sent_at DESC NULLS LAST LIMIT 1",
                [to.trim()],
                |r| r.get(0),
            )
            .optional()
    }

    /// People you've written to, most often first.
    pub fn contacts(&self, limit: usize) -> Result<Vec<Contact>, StoreError> {
        let mut q = self.db.prepare(
            "SELECT address, max(name), count(DISTINCT account || remote_id) n FROM sent_recipients
             WHERE address NOT IN (SELECT address FROM identities)
             GROUP BY address ORDER BY n DESC LIMIT ?1",
        )?;
        q.query_map([limit as i64], |r| {
            Ok(Contact {
                address: r.get(0)?,
                name: r.get(1)?,
                times: r.get(2)?,
            })
        })?
        .collect()
    }

    /// What a reply to message `id` needs.
    pub fn reply_context(&self, id: &str) -> Result<Option<ReplyContext>, StoreError> {
        let row: Option<(Vec<u8>, String, String)> = self
            .db
            .query_row(
                "SELECT headers, account, arrived_on FROM messages WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let Some((headers, account, arrived_on)) = row else {
            return Ok(None);
        };
        let Some(msg) = MessageParser::default().parse_headers(&headers) else {
            return Ok(None);
        };
        // Reply-To wins over From, as in every mail client.
        let reply_to = msg.reply_to().or(msg.from()).and_then(|a| a.first());
        let to = match (
            reply_to.and_then(|a| a.name()),
            reply_to.and_then(|a| a.address()),
        ) {
            // Quoted: real names may contain commas.
            (Some(n), Some(a)) => format!("\"{}\" <{a}>", n.replace('"', "'")),
            (None, Some(a)) => a.to_string(),
            _ => String::new(),
        };
        let mut references: Vec<String> = msg
            .references()
            .as_text_list()
            .map(|l| l.iter().map(|s| s.to_string()).collect())
            .unwrap_or_default();
        let message_id = msg.message_id().map(str::to_string);
        if let Some(mid) = &message_id {
            references.push(mid.clone());
        }
        Ok(Some(ReplyContext {
            account,
            arrived_on,
            to,
            subject: reply_subject(msg.subject().unwrap_or_default()),
            message_id,
            references,
        }))
    }

    pub fn set_body(&self, id: &str, text: &str) -> Result<(), StoreError> {
        self.db.execute(
            "INSERT INTO bodies (message, text) VALUES (?1, ?2)
             ON CONFLICT (message) DO UPDATE SET text = excluded.text",
            params![id, text],
        )?;
        Ok(())
    }
}

/// Re-sorts every message against the current identities, contacts and
/// decisions. Returns how many changed bucket or reason.
/// ponytail: full pass each time (headers are re-parsed); fine to ~100k messages,
/// then reclassify only rows touched by the change.
fn reclassify(db: &Connection) -> Result<usize, StoreError> {
    let mut ctx = Context::default();
    let mut q = db.prepare("SELECT address FROM identities")?;
    ctx.identities = q.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
    let mut q = db.prepare(
        "SELECT address, max(name), count(*) FROM sent_recipients
         WHERE address NOT IN (SELECT address FROM identities) GROUP BY address",
    )?;
    ctx.contacts = q
        .query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))?
        .collect::<Result<_, _>>()?;
    let mut q = db.prepare("SELECT address, decision FROM sender_decisions")?;
    ctx.decisions = q
        .query_map([], |r| {
            let d: String = r.get(1)?;
            Ok((r.get(0)?, parse_decision(&d).map_err(|e| conversion(1, e))?))
        })?
        .collect::<Result<_, _>>()?;

    let mut q = db.prepare("SELECT DISTINCT message_id FROM replied")?;
    ctx.replied = q.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;

    let mut q = db.prepare("SELECT name, lower(address) FROM accounts")?;
    let account_address: std::collections::HashMap<String, String> = q
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;

    let mut q = db.prepare("SELECT key FROM extracted WHERE kind = 'code'")?;
    let code_mail: std::collections::HashSet<String> =
        q.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;

    let mut q = db.prepare(&format!(
        "SELECT id, headers, bucket, reason, intent, replied, account, arrived_on, {DONE_KEY}
         FROM messages"
    ))?;
    type Row = (
        i64,
        Vec<u8>,
        String,
        String,
        String,
        bool,
        String,
        String,
        String,
    );
    let rows: Vec<Row> = q
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
            ))
        })?
        .collect::<Result<_, _>>()?;
    let mut changed = 0;
    for (id, headers, bucket, reason, intent, replied, account, arrived, key) in rows {
        // Codes and reset links never enter the inbox, whoever sent them.
        let (b, r) = if code_mail.contains(&key) {
            (Bucket::Codes, hysj_proto::Reason::OneTimeCode)
        } else {
            classify::classify(&headers, &ctx)
        };
        let r = serde_json::to_string(&r).expect("Reason serializes");
        let (i, rep) = classify::intent(&headers, &ctx);
        let on = classify::arrived_on(&headers, &ctx)
            .or_else(|| account_address.get(&account).cloned())
            .unwrap_or_default();
        if bucket_str(b) != bucket
            || r != reason
            || intent_str(i) != intent
            || rep != replied
            || on != arrived
        {
            db.execute(
                "UPDATE messages SET bucket = ?2, reason = ?3, intent = ?4, replied = ?5, arrived_on = ?6
                 WHERE id = ?1",
                params![id, bucket_str(b), r, intent_str(i), rep, on],
            )?;
            changed += 1;
        }
    }
    Ok(changed)
}

/// "Re: " once, keeping an existing prefix in any language ("SV:", "VS:", "AW:").
pub fn reply_subject(subject: &str) -> String {
    let lower = subject.trim_start().to_lowercase();
    if ["re:", "sv:", "vs:", "aw:"]
        .iter()
        .any(|p| lower.starts_with(p))
    {
        subject.trim().to_string()
    } else {
        format!("Re: {}", subject.trim())
    }
}

fn self_identity(
    db: &Connection,
    account: &str,
    address: &str,
    name: Option<&str>,
) -> Result<(), StoreError> {
    db.execute(
        "INSERT INTO identities (account, address, name) VALUES (?1, lower(?2), ?3)
         ON CONFLICT DO UPDATE SET name = coalesce(excluded.name, name)",
        params![account, address, name],
    )?;
    Ok(())
}

fn tags_json(tags: &[String]) -> String {
    serde_json::to_string(tags).expect("strings serialize")
}

/// Columns for [`summary_row`]; the last two are the newest accepted AI gist
/// and replies for the message (key as in `DONE_KEY`).
const SUMMARY_COLS: &str = "id, account, from_name, from_addr, subject, date, bucket, reason, tags, intent, replied, arrived_on,
    (SELECT result FROM ai_cache a WHERE a.key = coalesce(messages.message_id, messages.account || ':' || messages.remote_id)
        AND a.task = 'gist' AND a.ok = 1 ORDER BY a.at DESC LIMIT 1),
    (SELECT result FROM ai_cache a WHERE a.key = coalesce(messages.message_id, messages.account || ':' || messages.remote_id)
        AND a.task = 'replies' AND a.ok = 1 ORDER BY a.at DESC LIMIT 1)";

fn summary_row(r: &rusqlite::Row) -> rusqlite::Result<MessageSummary> {
    let from_addr: String = r.get(3)?;
    let date: Option<i64> = r.get(5)?;
    let bucket: String = r.get(6)?;
    Ok(MessageSummary {
        id: r.get::<_, i64>(0)?.to_string(),
        account: r.get(1)?,
        from: r
            .get::<_, Option<String>>(2)?
            .unwrap_or_else(|| from_addr.clone()),
        from_addr,
        subject: r.get(4)?,
        date: date.map(|t| mail_parser::DateTime::from_timestamp(t).to_rfc3339()),
        ts: date,
        bucket: parse_bucket(&bucket).map_err(|e| conversion(6, e))?,
        reason: serde_json::from_str(&r.get::<_, String>(7)?)
            .map_err(|e| conversion(7, e.to_string()))?,
        tags: serde_json::from_str(&r.get::<_, String>(8)?).unwrap_or_default(),
        to: None,
        gist: r.get(12)?,
        replies: r
            .get::<_, Option<String>>(13)?
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default(),
        intent: parse_intent(&r.get::<_, String>(9)?).map_err(|e| conversion(9, e))?,
        replied: r.get(10)?,
        arrived_on: r.get(11)?,
    })
}

fn bucket_str(b: Bucket) -> &'static str {
    match b {
        Bucket::People => "people",
        Bucket::Digest => "digest",
        Bucket::Screener => "screener",
        Bucket::Blocked => "blocked",
        Bucket::Codes => "codes",
    }
}

// Explicit both ways: an unknown value is an error, never silently the Screener.
fn parse_bucket(s: &str) -> Result<Bucket, String> {
    Ok(match s {
        "people" => Bucket::People,
        "digest" => Bucket::Digest,
        "screener" => Bucket::Screener,
        "blocked" => Bucket::Blocked,
        "codes" => Bucket::Codes,
        other => return Err(format!("unknown bucket {other:?}")),
    })
}

fn intent_str(i: Intent) -> &'static str {
    match i {
        Intent::Reply => "reply",
        Intent::Read => "read",
        Intent::Know => "know",
    }
}

fn parse_intent(s: &str) -> Result<Intent, String> {
    Ok(match s {
        "reply" => Intent::Reply,
        "read" => Intent::Read,
        "know" => Intent::Know,
        other => return Err(format!("unknown intent {other:?}")),
    })
}

fn decision_str(d: Decision) -> &'static str {
    match d {
        Decision::Allow => "allow",
        Decision::Digest => "digest",
        Decision::Block => "block",
    }
}

fn parse_decision(s: &str) -> Result<Decision, String> {
    Ok(match s {
        "allow" => Decision::Allow,
        "digest" => Decision::Digest,
        "block" => Decision::Block,
        other => return Err(format!("unknown decision {other:?}")),
    })
}

fn conversion(col: usize, msg: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(col, rusqlite::types::Type::Text, msg.into())
}

// --- Codes, bookings, orders ------------------------------------------------------

/// A message waiting for body extraction.
pub struct Pending {
    pub id: i64,
    pub account: String,
    pub remote_id: String,
    pub key: String,
    pub date: Option<i64>,
    pub candidate: crate::extract::Candidate,
}

/// ISO 8601 with or without offset -> unix seconds (no offset is read as UTC;
/// good enough for sorting and "upcoming").
pub fn iso_ts(s: &str) -> Option<i64> {
    if let Ok(t) = s.parse::<jiff::Timestamp>() {
        return Some(t.as_second());
    }
    if let Ok(dt) = s.parse::<jiff::civil::DateTime>() {
        return dt
            .to_zoned(jiff::tz::TimeZone::UTC)
            .ok()
            .map(|z| z.timestamp().as_second());
    }
    s.parse::<jiff::civil::Date>()
        .ok()
        .and_then(|d| d.to_zoned(jiff::tz::TimeZone::UTC).ok())
        .map(|z| z.timestamp().as_second())
}

impl Store {
    /// Messages worth a body fetch that haven't had one: codes first, then
    /// newest first. Never mail you sent.
    pub fn pending_extracts(
        &self,
        account: &str,
        limit: usize,
    ) -> Result<Vec<Pending>, StoreError> {
        let mut q = self.db.prepare(&format!(
            "SELECT id, account, remote_id, {DONE_KEY}, date, subject, from_addr FROM messages
             WHERE account = ?1
               AND {DONE_KEY} NOT IN (SELECT key FROM extracted)
               AND lower(from_addr) NOT IN (SELECT address FROM identities)
             ORDER BY date DESC"
        ))?;
        let mut out: Vec<Pending> = q
            .query_map([account], |r| {
                let subject: String = r.get(5)?;
                let from: String = r.get(6)?;
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    subject,
                    from,
                ))
            })?
            .filter_map(|row| {
                let (id, account, remote_id, key, date, subject, from) = row.ok()?;
                let candidate = if crate::extract::source(&from) {
                    crate::extract::Candidate::Source
                } else {
                    crate::extract::candidate(&subject)?
                };
                Some(Pending {
                    id,
                    account,
                    remote_id,
                    key,
                    date,
                    candidate,
                })
            })
            .collect();
        out.sort_by_key(|p| (p.candidate, std::cmp::Reverse(p.date)));
        out.truncate(limit);
        Ok(out)
    }

    /// Stores what was found in one message (maybe nothing) and re-sorts, so a
    /// code mail leaves the Screener/Digest in the same sync.
    pub fn record_extracts(
        &mut self,
        p: &Pending,
        found: &[crate::extract::Extract],
    ) -> Result<(), StoreError> {
        use crate::extract::{CodeKind, Extract};
        let tx = self.db.transaction()?;
        let date = p.date.unwrap_or(0);
        let mut kind = None;
        for e in found {
            match e {
                Extract::Code(c) => {
                    kind = Some("code");
                    let label = match c.kind {
                        CodeKind::SignIn => "Sign-in code",
                        CodeKind::Verification => "Email verification",
                        CodeKind::Code => "Code",
                        CodeKind::Reset => "Password reset",
                    };
                    // Valid from when it was sent, not from when we read it.
                    tx.execute(
                        "INSERT OR REPLACE INTO codes
                         (key, kind, service, sender_domain, code, link, link_host, link_ok, arrived, expires)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                        params![
                            p.key, label, c.service, c.sender_domain, c.code, c.link, c.link_host,
                            c.link_ok, date, date + i64::from(c.valid_minutes) * 60
                        ],
                    )?;
                }
                Extract::Booking(b) => {
                    kind.get_or_insert("structured");
                    merge_booking(&tx, b, &p.key, date)?;
                }
                Extract::Order(o) => {
                    kind.get_or_insert("structured");
                    merge_order(&tx, o, &p.key, date)?;
                }
                Extract::Listing(l) => {
                    kind.get_or_insert("listing");
                    // The same ad in a later alert: newest message, and a
                    // changed price keeps the one before.
                    tx.execute(
                        "INSERT INTO listings (key, message, kind, title, price, place, detail, link, image, first_seen)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                         ON CONFLICT (key) DO UPDATE SET
                             message = excluded.message, kind = excluded.kind, title = excluded.title, place = excluded.place,
                             detail = excluded.detail, image = excluded.image,
                             prev_price = CASE WHEN excluded.price != listings.price AND listings.price != ''
                                               THEN listings.price ELSE listings.prev_price END,
                             price = excluded.price",
                        params![
                            format!("finn:{}", l.ad), p.id, l.kind, l.title, l.price, l.place, l.detail,
                            l.link, l.image, date
                        ],
                    )?;
                }
            }
        }
        tx.execute(
            "INSERT OR REPLACE INTO extracted (key, kind, at) VALUES (?1, ?2, unixepoch())",
            params![p.key, kind],
        )?;
        if kind == Some("code") {
            // Never keep a code mail's text around.
            tx.execute("DELETE FROM bodies WHERE message = ?1", [p.id])?;
            reclassify(&tx)?;
        }
        tx.commit()
    }

    /// Masks expired codes to their last two digits and forgets them after
    /// `retention_hours`. ponytail: the mail itself stays on the server (and its
    /// subject/headers here) until archive/delete via the backend exists.
    pub fn expire_codes(&self, now: i64) -> Result<(), StoreError> {
        let hours = self.code_settings()?.retention_hours;
        self.db.execute(
            "UPDATE codes SET code = substr(code, -2) WHERE expires <= ?1 AND length(code) > 2",
            [now],
        )?;
        self.db.execute(
            "DELETE FROM codes WHERE expires <= ?1",
            [now - i64::from(hours) * 3600],
        )?;
        Ok(())
    }

    /// Live and recently expired codes, newest first.
    pub fn codes(&self, now: i64) -> Result<Vec<hysj_proto::CodeItem>, StoreError> {
        self.expire_codes(now)?;
        let mut q = self.db.prepare(&format!(
            "SELECT min(messages.id), c.kind, c.service, c.sender_domain, c.code, c.link, c.link_host,
                    c.link_ok, c.arrived, c.expires, c.reported
             FROM codes c JOIN messages ON {DONE_KEY} = c.key
             GROUP BY c.key ORDER BY c.arrived DESC"
        ))?;
        q.query_map([], |r| {
            let kind: String = r.get(1)?;
            Ok(hysj_proto::CodeItem {
                id: r.get::<_, i64>(0)?.to_string(),
                reset: kind == "Password reset",
                kind,
                service: r.get(2)?,
                sender_domain: r.get(3)?,
                code: r.get(4)?,
                link: r.get(5)?,
                link_host: r.get(6)?,
                link_ok: r.get(7)?,
                arrived: r.get(8)?,
                expires: r.get(9)?,
                reported: r.get(10)?,
            })
        })?
        .collect()
    }

    /// "I didn't ask for this": disables the reset link.
    pub fn report_code(&self, id: &str) -> Result<(), StoreError> {
        self.db.execute(
            &format!(
                "UPDATE codes SET reported = 1, link = NULL
                 WHERE key = (SELECT {DONE_KEY} FROM messages WHERE id = ?1)"
            ),
            [id],
        )?;
        Ok(())
    }

    pub fn code_settings(&self) -> Result<hysj_proto::CodeSettings, StoreError> {
        let get = |k: &str| -> Result<Option<String>, StoreError> {
            self.db
                .query_row("SELECT value FROM meta WHERE key = ?1", [k], |r| r.get(0))
                .optional()
        };
        Ok(hysj_proto::CodeSettings {
            popup: get("codes_popup")?.is_none_or(|v| v == "1"),
            retention_hours: get("codes_retention_hours")?
                .and_then(|v| v.parse().ok())
                .unwrap_or(24),
        })
    }

    pub fn set_code_settings(&self, s: &hysj_proto::CodeSettings) -> Result<(), StoreError> {
        let hours = if [1, 24, 168].contains(&s.retention_hours) {
            s.retention_hours
        } else {
            24
        };
        for (k, v) in [
            ("codes_popup", if s.popup { "1" } else { "0" }.to_string()),
            ("codes_retention_hours", hours.to_string()),
        ] {
            self.db.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
                params![k, v],
            )?;
        }
        Ok(())
    }

    pub fn bookings(&self) -> Result<Vec<hysj_proto::BookingItem>, StoreError> {
        let mut q = self.db.prepare(&format!(
            "SELECT b.key, kind, reference, title, start, end_at, start_ts, provider, city, seat, party, moved_from,
                    (SELECT count(*) FROM booking_sources s WHERE s.booking = b.key),
                    (SELECT messages.id FROM booking_sources s JOIN messages ON {DONE_KEY} = s.message
                     WHERE s.booking = b.key ORDER BY s.date DESC LIMIT 1)
             FROM bookings b ORDER BY start_ts"
        ))?;
        let list = q
            .query_map([], |r| {
                Ok(hysj_proto::BookingItem {
                    key: r.get(0)?,
                    kind: r.get(1)?,
                    reference: r.get(2)?,
                    title: r.get(3)?,
                    start: r.get(4)?,
                    end: r.get(5)?,
                    start_ts: r.get(6)?,
                    provider: r.get(7)?,
                    city: r.get(8)?,
                    seat: r.get(9)?,
                    party: r.get(10)?,
                    moved_from: r.get(11)?,
                    sources: r.get(12)?,
                    message: r.get::<_, Option<i64>>(13)?.map(|i| i.to_string()),
                    pass: None,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut sources = self.db.prepare(&format!(
            "SELECT messages.id FROM booking_sources s JOIN messages ON {DONE_KEY} = s.message
             WHERE s.booking = ?1 ORDER BY s.date DESC"
        ))?;
        let mut list = list;
        for b in &mut list {
            let ids: Vec<i64> = sources
                .query_map([&b.key], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            // ponytail: parses each source's stored original per call; few bookings.
            b.pass = ids.into_iter().find_map(|id| {
                let raw = self.raw(&id.to_string()).ok()??;
                let a = crate::view::view(&raw).attachments.into_iter().find(|a| {
                    a.mime == "application/vnd.apple.pkpass"
                        || a.name.to_lowercase().ends_with(".pkpass")
                })?;
                Some(hysj_proto::AttachmentParams {
                    id: id.to_string(),
                    index: a.index,
                })
            });
        }
        Ok(list)
    }

    pub fn orders(&self) -> Result<Vec<hysj_proto::OrderItem>, StoreError> {
        let mut q = self.db.prepare(&format!(
            "SELECT o.key, store, number, ordered, ordered_ts, items, total, status, eta, delivered_ts,
                    tracking_url, return_days,
                    (SELECT count(*) FROM order_sources s WHERE s.order_key = o.key),
                    (SELECT min(messages.id) FROM messages WHERE {DONE_KEY} = o.receipt)
             FROM orders o ORDER BY coalesce(ordered_ts, delivered_ts) DESC"
        ))?;
        q.query_map([], |r| {
            let items: String = r.get(5)?;
            let delivered: Option<i64> = r.get(9)?;
            let days: Option<i64> = r.get(11)?;
            Ok(hysj_proto::OrderItem {
                key: r.get(0)?,
                store: r.get(1)?,
                number: r.get(2)?,
                ordered: r.get(3)?,
                ordered_ts: r.get(4)?,
                items: serde_json::from_str(&items).unwrap_or_default(),
                total: r.get(6)?,
                status: r.get(7)?,
                eta: r.get(8)?,
                delivered_ts: delivered,
                tracking_url: r.get(10)?,
                return_by: delivered.zip(days).map(|(d, n)| d + n * 86_400),
                sources: r.get(12)?,
                receipt: r.get::<_, Option<i64>>(13)?.map(|i| i.to_string()),
            })
        })?
        .collect()
    }
}

/// Later mails update a booking in place; a changed start keeps the old one
/// as "Moved from".
fn merge_booking(
    db: &Connection,
    b: &crate::extract::Booking,
    message: &str,
    date: i64,
) -> Result<(), StoreError> {
    let old: Option<(Option<String>, i64)> = db
        .query_row(
            "SELECT start, updated FROM bookings WHERE key = ?1",
            [&b.key],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let newer = old.as_ref().is_none_or(|(_, updated)| date >= *updated);
    if newer {
        let moved = match &old {
            Some((Some(prev), _)) if b.start.as_ref().is_some_and(|s| s != prev) => {
                Some(prev.clone())
            }
            _ => None,
        };
        db.execute(
            "INSERT INTO bookings (key, kind, reference, title, start, end_at, start_ts, provider, city, seat, party, moved_from, updated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT (key) DO UPDATE SET
                 title = excluded.title, start = coalesce(excluded.start, start), end_at = coalesce(excluded.end_at, end_at),
                 start_ts = coalesce(excluded.start_ts, start_ts), provider = coalesce(excluded.provider, provider),
                 city = coalesce(excluded.city, city), seat = coalesce(excluded.seat, seat),
                 party = coalesce(excluded.party, party), moved_from = coalesce(excluded.moved_from, moved_from),
                 updated = excluded.updated",
            params![
                b.key, b.kind, b.reference, b.title, b.start, b.end, b.start.as_deref().and_then(iso_ts),
                b.provider, b.city, b.seat, b.party, moved, date
            ],
        )?;
    }
    db.execute(
        "INSERT OR IGNORE INTO booking_sources (booking, message, date) VALUES (?1, ?2, ?3)",
        params![b.key, message, date],
    )?;
    Ok(())
}

/// Confirmation, shipping and delivery mails move one order along; status only
/// goes forward.
fn merge_order(
    db: &Connection,
    o: &crate::extract::Order,
    message: &str,
    date: i64,
) -> Result<(), StoreError> {
    let items = serde_json::to_string(&o.items).expect("strings serialize");
    let delivered = (o.status == 4).then_some(date);
    let receipt = (!o.items.is_empty() || o.total.is_some()).then_some(message);
    db.execute(
        "INSERT INTO orders (key, store, number, ordered, ordered_ts, items, total, status, eta, delivered_ts, tracking_url, return_days, receipt)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT (key) DO UPDATE SET
             ordered = coalesce(ordered, excluded.ordered), ordered_ts = coalesce(ordered_ts, excluded.ordered_ts),
             items = CASE WHEN excluded.items != '[]' THEN excluded.items ELSE items END,
             total = coalesce(excluded.total, total), status = max(status, excluded.status),
             eta = coalesce(excluded.eta, eta), delivered_ts = coalesce(delivered_ts, excluded.delivered_ts),
             tracking_url = coalesce(excluded.tracking_url, tracking_url),
             return_days = coalesce(excluded.return_days, return_days), receipt = coalesce(receipt, excluded.receipt)",
        params![
            o.key, o.store, o.number, o.ordered, o.ordered.as_deref().and_then(iso_ts), items, o.total, o.status,
            o.eta, delivered, o.tracking_url, o.return_days, receipt
        ],
    )?;
    db.execute(
        "INSERT OR IGNORE INTO order_sources (order_key, message) VALUES (?1, ?2)",
        params![o.key, message],
    )?;
    Ok(())
}

// --- Local model results ------------------------------------------------------------

/// What the model should write for one message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiTask {
    Gist,
    /// A newsletter's gist (cached as a gist), plus whether it's an offer.
    Issue,
    Replies,
}

impl AiTask {
    pub fn name(self) -> &'static str {
        match self {
            AiTask::Gist | AiTask::Issue => "gist",
            AiTask::Replies => "replies",
        }
    }
}

/// A message waiting for the model.
#[derive(Debug, Clone)]
pub struct AiJob {
    pub id: String,
    pub key: String,
    pub task: AiTask,
    pub from: String,
    pub subject: String,
    /// `None` until the body has been fetched (lazy, like `show`).
    pub body: Option<String>,
}

/// How many times a failed or rejected answer is retried.
const AI_ATTEMPTS: i64 = 2;

impl Store {
    /// Work for the model, most useful first: people waiting on you (gist and
    /// replies), other mail from people (gist), then newsletters in the latest
    /// Digest delivery (gist). Never codes, blocked senders, your own or
    /// cleared mail. `delivery` is the latest Digest window `(after, upto]`.
    pub fn ai_jobs(
        &self,
        model: &str,
        delivery: (i64, i64),
        limit: usize,
    ) -> Result<Vec<AiJob>, StoreError> {
        let mut q = self.db.prepare(&format!(
            "SELECT id, {DONE_KEY}, coalesce(from_name, from_addr), subject, bucket, intent, reason, date,
                    (SELECT text FROM bodies WHERE bodies.message = messages.id)
             FROM messages
             WHERE bucket IN ('people', 'digest')
               AND {DONE_KEY} NOT IN (SELECT key FROM done)
               AND lower(from_addr) NOT IN (SELECT address FROM identities)
             ORDER BY date DESC"
        ))?;
        type Row = (
            i64,
            String,
            String,
            String,
            String,
            String,
            String,
            Option<i64>,
            Option<String>,
        );
        let rows: Vec<Row> = q
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                ))
            })?
            .collect::<Result<_, _>>()?;
        let mut tried = self.db.prepare(
            "SELECT 1 FROM ai_cache WHERE key = ?1 AND model = ?2 AND task = ?3 AND (ok = 1 OR attempts >= ?4)",
        )?;
        let mut jobs: Vec<(u8, AiJob)> = Vec::new();
        for (id, key, from, subject, bucket, intent, reason, date, body) in rows {
            let newsletter = reason.contains("\"bulk\"") && !reason.contains("Auto-Submitted");
            // Since the start of the latest edition, so the next one's are
            // written as they arrive, not when it's due.
            let recent = date.is_some_and(|t| t > delivery.0);
            let wanted: &[(u8, AiTask)] = match (bucket.as_str(), intent.as_str()) {
                ("people", "reply") => &[(0, AiTask::Gist), (0, AiTask::Replies)],
                ("people", _) => &[(1, AiTask::Gist)],
                ("digest", _) if newsletter && recent => &[(2, AiTask::Issue)],
                _ => &[],
            };
            for &(rank, task) in wanted {
                if tried.exists(params![key, model, task.name(), AI_ATTEMPTS])? {
                    continue;
                }
                let job = AiJob {
                    id: id.to_string(),
                    key: key.clone(),
                    task,
                    from: from.clone(),
                    subject: subject.clone(),
                    body: body.clone(),
                };
                jobs.push((rank, job));
            }
        }
        jobs.sort_by_key(|(rank, _)| *rank); // stable: newest first within a rank
        Ok(jobs.into_iter().take(limit).map(|(_, j)| j).collect())
    }

    /// Records an answer (`Ok`, shown) or a failure/rejection (`Err`, retried
    /// up to a limit).
    pub fn ai_store(
        &self,
        key: &str,
        model: &str,
        task: &str,
        result: Result<String, String>,
    ) -> Result<(), StoreError> {
        let (text, ok) = match result {
            Ok(t) => (Some(t), true),
            Err(_) => (None, false),
        };
        self.db.execute(
            "INSERT INTO ai_cache (key, model, task, result, ok, attempts, at) VALUES (?1, ?2, ?3, ?4, ?5, 1, unixepoch())
             ON CONFLICT (key, model, task) DO UPDATE SET
                 result = excluded.result, ok = excluded.ok, attempts = attempts + 1, at = excluded.at",
            params![key, model, task, text, ok],
        )?;
        Ok(())
    }

    /// The model's headline and lede for an edition, by [`crate::edition::facts_key`].
    pub fn digest_text(&self, key: &str) -> Result<Option<(String, String)>, StoreError> {
        let text: Option<String> = self
            .db
            .query_row(
                "SELECT result FROM ai_cache WHERE key = ?1 AND task = 'digest' AND ok = 1 ORDER BY at DESC LIMIT 1",
                [key],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        Ok(text.and_then(|t| serde_json::from_str(&t).ok()))
    }

    /// Whether an edition still needs its text written.
    pub fn digest_text_pending(&self, key: &str, model: &str) -> Result<bool, StoreError> {
        let done: bool = self.db.query_row(
            "SELECT EXISTS (SELECT 1 FROM ai_cache WHERE key = ?1 AND model = ?2 AND task = 'digest' AND (ok = 1 OR attempts >= ?3))",
            params![key, model, AI_ATTEMPTS],
            |r| r.get(0),
        )?;
        Ok(!done)
    }
}

// --- Original messages and images -----------------------------------------------------

/// Originals above this aren't kept: they're fetched again when needed.
pub const MAX_RAW: usize = 2 * 1024 * 1024;

impl Store {
    /// Keeps a message's original, unless it's code mail (whose text is never
    /// kept) or too large.
    pub fn set_raw(&self, id: &str, raw: &[u8]) -> Result<(), StoreError> {
        if raw.len() > MAX_RAW || self.is_code(id)? {
            return Ok(());
        }
        self.db.execute(
            "UPDATE bodies SET raw = ?2 WHERE message = ?1",
            params![id, raw],
        )?;
        Ok(())
    }

    pub fn raw(&self, id: &str) -> Result<Option<Vec<u8>>, StoreError> {
        Ok(self
            .db
            .query_row("SELECT raw FROM bodies WHERE message = ?1", [id], |r| {
                r.get(0)
            })
            .optional()?
            .flatten())
    }

    /// One-time codes and reset links: their text is never kept.
    pub fn is_code(&self, id: &str) -> Result<bool, StoreError> {
        Ok(self.db.query_row(
            &format!(
                "SELECT bucket = 'codes' OR EXISTS (SELECT 1 FROM extracted e WHERE e.key = {DONE_KEY} AND e.kind = 'code')
                 FROM messages WHERE id = ?1"
            ),
            [id],
            |r| r.get(0),
        ).optional()?.unwrap_or(false))
    }

    /// Whether remote images load for mail from `address`.
    pub fn images_allowed(&self, address: &str) -> Result<bool, StoreError> {
        self.db.query_row(
            "SELECT EXISTS (SELECT 1 FROM image_senders WHERE address = lower(?1))",
            [address],
            |r| r.get(0),
        )
    }

    pub fn set_images_allowed(&self, address: &str, allow: bool) -> Result<(), StoreError> {
        if allow {
            self.db.execute(
                "INSERT OR IGNORE INTO image_senders (address) VALUES (lower(?1))",
                [address],
            )?;
        } else {
            self.db.execute(
                "DELETE FROM image_senders WHERE address = lower(?1)",
                [address],
            )?;
        }
        Ok(())
    }
}

// --- Listings ----------------------------------------------------------------------------

/// A listing read from alert mail, as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredListing {
    pub key: String,
    /// The latest message it was in.
    pub message: String,
    pub kind: String,
    pub title: String,
    pub price: String,
    pub place: String,
    pub detail: String,
    pub link: String,
    pub image: Option<String>,
    pub first_seen: i64,
    pub prev_price: Option<String>,
}

impl Store {
    /// Listings last seen in any of these messages.
    pub fn listings_in(&self, message_ids: &[String]) -> Result<Vec<StoredListing>, StoreError> {
        let mut q = self.db.prepare(
            "SELECT key, message, kind, title, price, place, detail, link, image, first_seen, prev_price
             FROM listings WHERE message IN (SELECT value FROM json_each(?1)) ORDER BY first_seen DESC, key",
        )?;
        let ids: Vec<i64> = message_ids.iter().filter_map(|i| i.parse().ok()).collect();
        let ids = serde_json::to_string(&ids).expect("numbers");
        let rows = q.query_map([ids], |r| {
            Ok(StoredListing {
                key: r.get(0)?,
                message: r.get::<_, i64>(1)?.to_string(),
                kind: r.get(2)?,
                title: r.get(3)?,
                price: r.get(4)?,
                place: r.get(5)?,
                detail: r.get(6)?,
                link: r.get(7)?,
                image: r.get(8)?,
                first_seen: r.get(9)?,
                prev_price: r.get(10)?,
            })
        })?;
        rows.collect()
    }
}

// --- Senders sorted by the model ---------------------------------------------------------

impl Store {
    /// Bulk senders the model hasn't sorted yet (shop or publication), busiest
    /// first: (address, name, recent subjects).
    pub fn senders_to_sort(
        &self,
        model: &str,
        limit: usize,
    ) -> Result<Vec<(String, String, Vec<String>)>, StoreError> {
        let mut q = self.db.prepare(
            "SELECT lower(from_addr), max(from_name), count(*) n,
                    (SELECT json_group_array(subject) FROM (SELECT subject FROM messages m2
                       WHERE lower(m2.from_addr) = lower(messages.from_addr) ORDER BY date DESC LIMIT 6))
             FROM messages
             WHERE bucket = 'digest' AND reason LIKE '%\"bulk\"%' AND reason NOT LIKE '%Auto-Submitted%'
               AND NOT EXISTS (SELECT 1 FROM ai_cache a WHERE a.key = 'sender:' || lower(messages.from_addr)
                               AND a.model = ?1 AND a.task = 'sender_kind' AND (a.ok = 1 OR a.attempts >= 2))
             GROUP BY lower(from_addr) ORDER BY n DESC LIMIT ?2",
        )?;
        let rows = q.query_map(params![model, limit as i64], |r| {
            let subjects: String = r.get(3)?;
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                serde_json::from_str(&subjects).unwrap_or_default(),
            ))
        })?;
        rows.collect()
    }

    /// Senders the model sorted as shops.
    pub fn model_shops(&self) -> Result<std::collections::HashSet<String>, StoreError> {
        let mut q = self.db.prepare(
            "SELECT substr(key, 8) FROM ai_cache WHERE task = 'sender_kind' AND ok = 1 AND result = 'shop'",
        )?;
        let rows = q.query_map([], |r| r.get(0))?;
        rows.collect()
    }
}

// --- Bookings read by the model ----------------------------------------------------------

/// (message id, key, from, subject, body if fetched).
/// A mail the model may read for a booking or order.
pub struct BookingCandidate {
    pub id: String,
    pub key: String,
    pub from: String,
    pub addr: String,
    pub subject: String,
    /// `None` until fetched (lazy, like `show`).
    pub body: Option<String>,
    pub sent: Option<i64>,
    /// What the classifier says it is (booking, order, subscription).
    pub kind: String,
}

impl Store {
    /// Mail the classifier is sure is a booking, order or subscription and
    /// the rules found nothing in (no schema.org data), not yet read by the model.
    pub fn booking_candidates(
        &self,
        model: &str,
        limit: usize,
    ) -> Result<Vec<BookingCandidate>, StoreError> {
        let mut q = self.db.prepare(&format!(
            "SELECT id, {DONE_KEY}, coalesce(from_name, from_addr), subject,
                    (SELECT text FROM bodies WHERE bodies.message = messages.id), date, from_addr, k.result
             FROM messages JOIN ai_cache k ON k.key = {DONE_KEY} AND k.task = 'kind' AND k.ok = 1
             LEFT JOIN extracted e ON e.key = {DONE_KEY}
             WHERE e.kind IS NULL AND k.result IN ('booking', 'order', 'subscription')
               AND NOT EXISTS (SELECT 1 FROM ai_cache a WHERE a.key = {DONE_KEY} AND a.model = ?1
                               AND a.task = 'booking' AND (a.ok = 1 OR a.attempts >= 2))
             GROUP BY messages.id ORDER BY date DESC LIMIT ?2"
        ))?;
        let rows = q.query_map(params![model, limit as i64], |r| {
            Ok(BookingCandidate {
                id: r.get::<_, i64>(0)?.to_string(),
                key: r.get(1)?,
                from: r.get(2)?,
                subject: r.get(3)?,
                body: r.get(4)?,
                sent: r.get(5)?,
                addr: r.get(6)?,
                kind: r.get(7)?,
            })
        })?;
        rows.collect()
    }

    /// Digest mail the classifier hasn't sorted yet (with `model`'s vectors),
    /// newest first; FINN alerts, codes and schema.org mail are the rules'.
    pub fn unsorted(&self, model: &str, limit: usize) -> Result<Vec<BookingCandidate>, StoreError> {
        let mut q = self.db.prepare(&format!(
            "SELECT id, {DONE_KEY}, coalesce(from_name, from_addr), subject,
                    (SELECT text FROM bodies WHERE bodies.message = messages.id), date, from_addr
             FROM messages LEFT JOIN extracted e ON e.key = {DONE_KEY}
             WHERE bucket = 'digest' AND e.kind IS NULL
               AND NOT EXISTS (SELECT 1 FROM ai_cache a WHERE a.key = {DONE_KEY} AND a.model = ?1
                               AND a.task = 'kind' AND (a.ok = 1 OR a.attempts >= 2))
             ORDER BY date DESC LIMIT ?2"
        ))?;
        let rows = q.query_map(params![model, limit as i64], |r| {
            Ok(BookingCandidate {
                id: r.get::<_, i64>(0)?.to_string(),
                key: r.get(1)?,
                from: r.get(2)?,
                subject: r.get(3)?,
                body: r.get(4)?,
                sent: r.get(5)?,
                addr: r.get(6)?,
                kind: String::new(),
            })
        })?;
        rows.collect()
    }

    /// Whether a sender has sent a booking, order or subscription (as sorted):
    /// then all their mail is read in full ("Nå er det nært" is a booking's).
    pub fn sender_has_kind(&self, addr: &str) -> Result<bool, StoreError> {
        self.db.query_row(
            &format!(
                "SELECT EXISTS (SELECT 1 FROM messages JOIN ai_cache a ON a.key = {DONE_KEY}
                 WHERE lower(messages.from_addr) = lower(?1) AND a.task = 'kind'
                   AND a.result IN ('booking', 'order', 'subscription'))"
            ),
            [addr],
            |r| r.get(0),
        )
    }

    /// Sorts a sender's mail that the first pass skipped again (see
    /// [`Self::sender_has_kind`]).
    pub fn resort_sender(&self, addr: &str) -> Result<(), StoreError> {
        self.db.execute(
            &format!(
                "DELETE FROM ai_cache WHERE task = 'kind' AND result = 'skipped' AND key IN (
                     SELECT {DONE_KEY} FROM messages WHERE lower(from_addr) = lower(?1))"
            ),
            [addr],
        )?;
        Ok(())
    }

    /// A stored sentence vector.
    pub fn vector(&self, key: &str, model: &str) -> Result<Option<Vec<f32>>, StoreError> {
        Ok(self
            .db
            .query_row(
                "SELECT vec FROM vectors WHERE key = ?1 AND model = ?2",
                params![key, model],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()?
            .map(|b| crate::kinds::from_blob(&b)))
    }

    pub fn put_vector(&self, key: &str, model: &str, v: &[f32]) -> Result<(), StoreError> {
        self.db.execute(
            "INSERT OR REPLACE INTO vectors (key, model, vec) VALUES (?1, ?2, ?3)",
            params![key, model, crate::kinds::to_blob(v)],
        )?;
        Ok(())
    }

    /// The reader's corrections: (message key, kind).
    pub fn kind_labels(&self) -> Result<Vec<(String, String)>, StoreError> {
        let mut q = self
            .db
            .prepare("SELECT key, kind FROM kind_labels ORDER BY at")?;
        let rows = q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect()
    }

    /// "This isn't a booking, it's a subscription": the booking or order
    /// `record` (its key) is removed, the mails it came from are labelled
    /// `kind` for the classifier and sorted again. Returns the mails' keys.
    pub fn correct_kind(&mut self, record: &str, kind: &str) -> Result<Vec<String>, StoreError> {
        let tx = self.db.transaction()?;
        let messages: Vec<String> = {
            let mut q = tx.prepare(
                "SELECT message FROM booking_sources WHERE booking = ?1
                 UNION SELECT message FROM order_sources WHERE order_key = ?1",
            )?;
            q.query_map([record], |r| r.get(0))?
                .collect::<Result<_, _>>()?
        };
        for m in &messages {
            tx.execute(
                "INSERT OR REPLACE INTO kind_labels (key, kind, at) VALUES (?1, ?2, unixepoch())",
                params![m, kind],
            )?;
            tx.execute(
                "DELETE FROM ai_cache WHERE key = ?1 AND task IN ('kind', 'booking')",
                [m],
            )?;
        }
        tx.execute("DELETE FROM bookings WHERE key = ?1", [record])?;
        tx.execute("DELETE FROM booking_sources WHERE booking = ?1", [record])?;
        tx.execute("DELETE FROM orders WHERE key = ?1", [record])?;
        tx.execute("DELETE FROM order_sources WHERE order_key = ?1", [record])?;
        tx.commit()?;
        Ok(messages)
    }

    /// Files a booking found by the model, merged like any other.
    pub fn record_booking(
        &mut self,
        message_key: &str,
        b: &crate::extract::Booking,
        date: i64,
    ) -> Result<(), StoreError> {
        let tx = self.db.transaction()?;
        merge_booking(&tx, b, message_key, date)?;
        // A payment or receipt with the same number, read before, is part of it.
        tx.execute(
            "INSERT OR IGNORE INTO booking_sources (booking, message, date)
             SELECT ?1, s.message, ?3 FROM order_sources s JOIN orders o ON o.key = s.order_key
             WHERE o.number = ?2",
            params![b.key, b.reference, date],
        )?;
        tx.execute(
            "DELETE FROM order_sources WHERE order_key IN (SELECT key FROM orders WHERE number = ?1)",
            [&b.reference],
        )?;
        tx.execute("DELETE FROM orders WHERE number = ?1", [&b.reference])?;
        tx.commit()
    }

    /// Files a mail under the booking with this number, if there is one: a
    /// confirmation without dates, or the payment for it.
    /// ponytail: numbers match across senders; require the same sender if they collide.
    pub fn link_booking(
        &self,
        number: &str,
        message_key: &str,
        date: i64,
    ) -> Result<bool, StoreError> {
        self.db.execute(
            "INSERT OR IGNORE INTO booking_sources (booking, message, date)
             SELECT key, ?2, ?3 FROM bookings WHERE reference = ?1",
            params![number, message_key, date],
        )?;
        self.db.query_row(
            "SELECT EXISTS (SELECT 1 FROM bookings WHERE reference = ?1)",
            [number],
            |r| r.get(0),
        )
    }

    /// What the model made of a sender's earlier mail, for its prompt:
    /// "3 × purchase, 1 × booking; recent subjects: …". Empty for a new sender.
    pub fn sender_history(&self, addr: &str, except: &str) -> Result<String, StoreError> {
        let mut q = self.db.prepare(&format!(
            "SELECT a.result FROM ai_cache a JOIN messages ON a.key = {DONE_KEY}
             WHERE lower(messages.from_addr) = lower(?1) AND a.key != ?2
               AND a.task = 'booking' AND a.ok = 1"
        ))?;
        let mut counts: Vec<(String, u32)> = Vec::new();
        for r in q.query_map(params![addr, except], |r| r.get::<_, String>(0))? {
            let r = r?;
            // "order:purchase", "none:offer", "linked:booking", "booking".
            let what = r.rsplit(':').next().unwrap_or_default().to_string();
            match counts.iter_mut().find(|(w, _)| *w == what) {
                Some((_, n)) => *n += 1,
                None => counts.push((what, 1)),
            }
        }
        counts.sort_by_key(|c| std::cmp::Reverse(c.1));
        let mut q = self.db.prepare(&format!(
            "SELECT subject FROM messages WHERE lower(from_addr) = lower(?1) AND {DONE_KEY} != ?2
             ORDER BY date DESC LIMIT 5"
        ))?;
        let subjects: Vec<String> = q
            .query_map(params![addr, except], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        if subjects.is_empty() {
            return Ok(String::new());
        }
        let read = counts
            .iter()
            .map(|(w, n)| format!("{n} × {w}"))
            .collect::<Vec<_>>()
            .join(", ");
        let subjects = subjects
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(", ");
        Ok(if read.is_empty() {
            format!("recent subjects: {subjects}")
        } else {
            format!("{read}; recent subjects: {subjects}")
        })
    }

    /// An order the model read (see [`crate::extract::checked_order`]).
    pub fn record_order(
        &mut self,
        message_key: &str,
        o: &crate::extract::Order,
        date: i64,
    ) -> Result<(), StoreError> {
        let tx = self.db.transaction()?;
        merge_order(&tx, o, message_key, date)?;
        tx.commit()
    }
}
