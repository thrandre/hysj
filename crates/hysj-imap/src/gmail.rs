//! Gmail over IMAP: All Mail is the one mailbox; labels become tags.
//!
//! ponytail: threads (X-GM-THRID) are parsed by imap-proto but async-imap has no
//! accessor; build threads from References/In-Reply-To when the Desk needs them (step 5).
//! Non-ASCII labels arrive as modified UTF-7 and are stored as-is for now.

use async_imap::types::Fetch;

/// `X-GM-LABELS`, e.g. `\Inbox`, `\Sent`, `\Important`, user labels.
pub(crate) fn labels(f: &Fetch) -> Vec<String> {
    f.gmail_labels()
        .map(|l| l.iter().map(|s| s.to_string()).collect())
        .unwrap_or_default()
}
