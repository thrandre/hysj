use std::future::Future;

pub type BackendError = Box<dyn std::error::Error + Send + Sync>;

/// One mail server connection (IMAP today; JMAP / Gmail API later).
/// Cursors and remote ids are opaque strings owned by the backend.
pub trait MailBackend: Send {
    /// New messages since `cursor` (`None` = first sync), headers only.
    fn sync_since(
        &mut self,
        cursor: Option<&str>,
    ) -> impl Future<Output = Result<Changes, BackendError>> + Send;

    /// The full raw message (RFC 5322 bytes).
    fn fetch(
        &mut self,
        remote_id: &str,
    ) -> impl Future<Output = Result<Vec<u8>, BackendError>> + Send;

    /// The first `max_bytes` of several raw messages, for extraction. Missing
    /// ones (deleted meanwhile) are left out.
    fn fetch_many(
        &mut self,
        remote_ids: &[String],
        max_bytes: u32,
    ) -> impl Future<Output = Result<Vec<(String, Vec<u8>)>, BackendError>> + Send;

    /// Sends, and files a copy in Sent where the server doesn't. Returns the Message-ID.
    /// Mail on the server matching `query` (Gmail search syntax on Gmail, text
    /// elsewhere), newest first, headers only — including mail older than what
    /// sync keeps.
    fn search(
        &mut self,
        query: &str,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<RawHeaders>, BackendError>> + Send;

    fn send(&mut self, msg: &Outgoing)
    -> impl Future<Output = Result<String, BackendError>> + Send;
}

#[derive(Debug, Default)]
pub struct Changes {
    pub cursor: String,
    /// The old cursor is no longer valid (e.g. IMAP UIDVALIDITY changed):
    /// drop everything stored for this account before applying `new`.
    pub reset: bool,
    pub new: Vec<RawHeaders>,
    /// New tag sets for already-synced messages: `(remote_id, tags)`.
    pub retagged: Vec<(String, Vec<String>)>,
    /// Every remote id still on the server within the synced range. Stored
    /// messages missing from it were deleted. `None` = no deletion info.
    pub present: Option<Vec<String>>,
    /// Newly seen mail the user sent: From/To/Cc/Bcc/Date/In-Reply-To/References only.
    /// Feeds identities, "have I emailed them" (contacts), "already replied" and
    /// the From suggestion (who sent it, when).
    pub sent: Vec<RawHeaders>,
    /// The sent history is being re-sent from scratch: drop what we had.
    pub sent_reset: bool,
}

#[derive(Debug, Default, Clone)]
pub struct RawHeaders {
    pub remote_id: String,
    pub headers: Vec<u8>,
    /// Gmail labels (`\Inbox`, `\Sent`, user labels); empty elsewhere.
    pub tags: Vec<String>,
    /// Provider's stable message id (Gmail `X-GM-MSGID`).
    pub provider_id: Option<String>,
}

/// A message to send. Addresses may carry names: `Aisha <aisha@example.com>`.
#[derive(Debug, Clone, Default)]
pub struct Outgoing {
    pub from: String,
    pub from_name: Option<String>,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub body: String,
    /// Message-ID being answered (without `<>`).
    pub in_reply_to: Option<String>,
    /// Thread ancestry, oldest first (without `<>`).
    pub references: Vec<String>,
}

/// Tag marking a message the user sent (Gmail system label).
pub const SENT_TAG: &str = "\\Sent";
