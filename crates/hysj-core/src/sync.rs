//! Backend-agnostic sync: pull changes into the store, load bodies lazily.

use mail_parser::MessageParser;

use crate::backend::{BackendError, MailBackend};
use crate::store::{Store, Stored};

/// Pulls everything new for `account`. Returns the number of new messages.
pub async fn sync_account(
    store: &mut Store,
    backend: &mut impl MailBackend,
    account: &str,
) -> Result<usize, BackendError> {
    let cursor = store.cursor(account)?;
    let changes = backend.sync_since(cursor.as_deref()).await?;
    Ok(store.apply(account, &changes)?)
}

/// The message with its body, fetching and caching the body on first view.
pub async fn load(
    store: &mut Store,
    backend: &mut impl MailBackend,
    id: &str,
) -> Result<Option<Stored>, BackendError> {
    let Some(mut msg) = store.get(id)? else {
        return Ok(None);
    };
    if msg.body.is_none() {
        let raw = backend.fetch(&msg.remote_id).await?;
        let text = body_text(&raw);
        store.set_body(id, &text)?;
        store.set_raw(id, &raw)?;
        msg.body = Some(text);
    }
    Ok(Some(msg))
}

/// The original message: kept from the first view, else fetched again.
pub async fn raw(
    store: &mut Store,
    backend: &mut impl MailBackend,
    id: &str,
) -> Result<Option<Vec<u8>>, BackendError> {
    if let Some(r) = store.raw(id)? {
        return Ok(Some(r));
    }
    let Some(msg) = store.get(id)? else {
        return Ok(None);
    };
    let raw = backend.fetch(&msg.remote_id).await?;
    if msg.body.is_none() {
        store.set_body(id, &body_text(&raw))?;
    }
    store.set_raw(id, &raw)?;
    Ok(Some(raw))
}

/// Plain-text body; HTML-only mail is converted by mail-parser.
fn body_text(raw: &[u8]) -> String {
    MessageParser::default()
        .parse(raw)
        .and_then(|m| m.body_text(0).map(|t| t.into_owned()))
        .unwrap_or_default()
}

/// Bodies fetched per sync for extraction, so a sync stays quick on slow
/// links; the backlog drains over later syncs.
pub const EXTRACT_PER_SYNC: usize = 25;
/// Enough for codes and JSON-LD near the top; truncated mail is fine.
pub const EXTRACT_MAX_BYTES: u32 = 256 * 1024;

/// Fetches bodies of candidate mails (codes first) and stores what the rules
/// find. Returns how many messages were examined.
pub async fn extract_pending(
    store: &mut Store,
    backend: &mut impl MailBackend,
    account: &str,
) -> Result<usize, BackendError> {
    let pending = store.pending_extracts(account, EXTRACT_PER_SYNC)?;
    if pending.is_empty() {
        return Ok(0);
    }
    let ids: Vec<String> = pending.iter().map(|p| p.remote_id.clone()).collect();
    let bodies: std::collections::HashMap<String, Vec<u8>> = backend
        .fetch_many(&ids, EXTRACT_MAX_BYTES)
        .await?
        .into_iter()
        .collect();
    for p in &pending {
        // A message gone from the server is recorded as examined, not retried.
        let found = bodies
            .get(&p.remote_id)
            .map(|raw| crate::extract::extract(raw))
            .unwrap_or_default();
        store.record_extracts(p, &found)?;
    }
    Ok(pending.len())
}
