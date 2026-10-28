//! Wire types shared by `hysjd` and its clients: JSON-RPC 2.0 envelopes,
//! the version handshake, and per-method params/results.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Bumped on any incompatible change to the types below.
pub const PROTOCOL_VERSION: u32 = 27;

// --- JSON-RPC 2.0 envelopes -------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

impl Request {
    pub fn new(id: u64, method: &str, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub jsonrpc: String,
    /// `None` only when the request couldn't be parsed far enough to read its id.
    pub id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    pub fn ok(id: u64, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: Some(id),
            result: Some(result),
            error: None,
        }
    }
    pub fn err(id: Option<u64>, error: RpcError) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

impl RpcError {
    pub const PARSE: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const VERSION_MISMATCH: i32 = -32000;
    pub const HELLO_REQUIRED: i32 = -32001;
    pub const NOT_FOUND: i32 = -32002;
    /// Mail server, keychain or store failure.
    pub const BACKEND: i32 = -32003;

    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

impl std::error::Error for RpcError {}

/// Daemon → client push (no id). Clients react by calling `<type>/changes`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateChanged {
    #[serde(rename = "type")]
    pub kind: String,
    pub state: u64,
}

// --- Methods ----------------------------------------------------------------

pub mod method {
    pub const HELLO: &str = "hello";
    pub const LIST: &str = "list";
    pub const SHOW: &str = "show";
    pub const SYNC: &str = "sync";
    pub const ACCOUNT_ADD: &str = "account/add";
    pub const ACCOUNT_LIST: &str = "account/list";
    /// Who an address belongs to, how it signs in, and its servers.
    pub const ACCOUNT_DISCOVER: &str = "account/discover";
    /// "Sign in with Microsoft": the page to open in the browser.
    pub const ACCOUNT_OAUTH_START: &str = "account/oauth_start";
    /// Waits for that sign-in to finish, then adds the account.
    pub const ACCOUNT_OAUTH_FINISH: &str = "account/oauth_finish";
    /// Rename an account as shown (its key stays).
    pub const ACCOUNT_LABEL: &str = "account/label";
    pub const IDENTITY_LIST: &str = "identity/list";
    pub const SCREENER_DECIDE: &str = "screener/decide";
    pub const SCREENER_FORGET: &str = "screener/forget";
    pub const MESSAGE_DONE: &str = "message/done";
    pub const MESSAGE_RESTORE: &str = "message/restore";
    pub const FOLDER_LIST: &str = "folder/list";
    pub const CODE_LIST: &str = "code/list";
    /// "I didn't ask for this" on a reset link.
    pub const CODE_REPORT: &str = "code/report";
    pub const CODE_SETTINGS: &str = "code/settings";
    /// Held mail that can't wait for the digest (failed payments, security
    /// alerts, review requests), for the Desk.
    pub const SIGNAL_LIST: &str = "signal/list";
    /// Hold (or stop holding) one kind of signal for the digest.
    pub const SIGNAL_HOLD: &str = "signal/hold";
    pub const BOOKING_LIST: &str = "booking/list";
    pub const ORDER_LIST: &str = "order/list";
    pub const SEND: &str = "send";
    /// A message ready to show: sanitised HTML, attachments, images.
    pub const MESSAGE_VIEW: &str = "message/view";
    /// One attachment's bytes.
    pub const MESSAGE_ATTACHMENT: &str = "message/attachment";
    /// Search mail on the server, older mail included.
    pub const SEARCH: &str = "search";
    /// A question about your mail, answered on this computer from the mail kept here.
    pub const ASK: &str = "ask";
    /// A Digest delivery as a short read: stories built from the mail in it.
    pub const DIGEST_EDITION: &str = "digest/edition";
    /// Whether the on-device AI runs; with params, turn it on or off first.
    pub const AI_SETTINGS: &str = "ai/settings";
    /// "Not a booking": what a booking's or order's mail really is (a kind in
    /// `hysj_core::kinds`). The card goes; the classifier learns from it.
    pub const KIND_CORRECT: &str = "kind/correct";
    /// The digest's delivery hours; with `hours`, set them first.
    pub const DIGEST_HOURS: &str = "digest/hours";
    /// Change a Digest setting offered in an edition (see `Control`).
    pub const DIGEST_SET: &str = "digest/set";
    /// Load (or stop loading) remote images in mail from a sender.
    pub const SENDER_IMAGES: &str = "sender/images";
    pub const CONTACT_LIST: &str = "contact/list";
    pub const IDENTITY_SUGGEST: &str = "identity/suggest";
    /// Notification method carrying [`super::StateChanged`].
    pub const STATE: &str = "state";
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hello {
    pub version: u32,
    /// "app" or "cli". The daemon posts OS notifications itself only when no
    /// app is connected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
}

/// Daemon → client push, no id: `{"jsonrpc":"2.0","method":"state","params":{...}}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub jsonrpc: String,
    pub method: String,
    pub params: StateChanged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloResult {
    pub version: u32,
    pub daemon: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum Bucket {
    People,
    Digest,
    Screener,
    /// Senders you blocked. Never shown or notified; listed only on request.
    Blocked,
    /// One-time codes and reset links: never in the inbox, shown in Codes.
    Codes,
}

/// Desk column for mail from people.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum Intent {
    /// Someone is waiting on you.
    Reply,
    /// No reply needed.
    Read,
    /// Glance and clear.
    Know,
}

/// A Screener decision about a sender. Global across accounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Allow,
    Digest,
    Block,
}

/// Why a message is where it is. Machine-readable; [`Reason::text`] renders it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reason {
    Decision {
        decision: Decision,
    },
    /// A bulk-mail header, e.g. `List-Unsubscribe`.
    Bulk {
        signal: String,
    },
    NoReply,
    EmailedBefore {
        times: u32,
    },
    /// Someone you've emailed is on the message (To/Cc).
    Introduced {
        by: String,
    },
    /// A one-time code or password reset (see Codes).
    OneTimeCode,
    /// Sent through an email delivery service (Amazon SES, SendGrid, ...) by
    /// someone you've never written to.
    DeliveryService {
        service: String,
    },
    /// A receipt or order confirmation from someone you've never written to.
    Receipt,
    FirstTime,
}

impl Reason {
    pub fn text(&self) -> String {
        match self {
            Reason::Decision {
                decision: Decision::Allow,
            } => "You let them in".into(),
            Reason::Decision {
                decision: Decision::Digest,
            } => "You sent them to the Digest".into(),
            Reason::Decision {
                decision: Decision::Block,
            } => "You blocked them".into(),
            Reason::Bulk { signal } => format!("Bulk mail ({signal})"),
            Reason::NoReply => "Sent from a no-reply address".into(),
            Reason::EmailedBefore { times: 1 } => "You've emailed them before".into(),
            Reason::EmailedBefore { times } => format!("You've emailed them {times} times"),
            Reason::Introduced { by } => format!("Introduced by {by}"),
            Reason::OneTimeCode => "One-time code or reset link".into(),
            Reason::DeliveryService { service } => format!("Sent through {service}"),
            Reason::Receipt => "A receipt or order confirmation".into(),
            Reason::FirstTime => "First-time sender".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ListParams {
    pub bucket: Bucket,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub account: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct MessageSummary {
    pub id: String,
    pub account: String,
    /// Display name, or the address when there is none.
    pub from: String,
    pub from_addr: String,
    pub subject: String,
    /// RFC 3339.
    pub date: Option<String>,
    /// Unix seconds, for "waiting 2 d".
    pub ts: Option<i64>,
    pub bucket: Bucket,
    pub reason: Reason,
    /// Desk column. ponytail: To/Cc heuristic until the AI tier (step 9).
    pub intent: Intent,
    /// You already replied to this (or a later message in its thread).
    pub replied: bool,
    /// Which of your addresses it was sent to: replies go out from here.
    pub arrived_on: String,
    /// One-sentence gist by the local model; `None` = show the subject.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub gist: Option<String>,
    /// Suggested replies by the local model (Reply cards only).
    #[serde(default)]
    pub replies: Vec<String>,
    /// Recipients, filled in for the Sent folder only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub to: Option<String>,
    /// Provider labels (Gmail `\Inbox`, user labels).
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ShowParams {
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Message {
    #[serde(flatten)]
    pub summary: MessageSummary,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Account {
    /// Short name used for filtering (`--account work`).
    pub name: String,
    pub address: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub smtp_host: String,
    pub smtp_port: u16,
    /// What the reader calls it ("Work"); the UI shows `name` without one.
    #[serde(default)]
    pub label: Option<String>,
}

/// What the add-account form needs to know about an address.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ProviderInfo {
    /// "gmail", "microsoft", "icloud", "yahoo", "fastmail" or "generic".
    pub id: String,
    /// "Gmail"; for an unknown domain, the domain.
    pub name: String,
    /// A provider Hysj knows (else the servers are a guess).
    pub known: bool,
    /// "password", "app_password", "oauth" or "command_line" (not added from
    /// the app: `help_label` says why, `help_steps` how).
    pub auth: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: String,
    pub smtp_port: u16,
    /// Where to make an app password, the link's name, and the steps.
    pub help_url: Option<String>,
    pub help_label: Option<String>,
    pub help_steps: Vec<String>,
    /// "Microsoft", for "Sign in with Microsoft".
    pub sign_in_with: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AccountOAuth {
    pub address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct OAuthStarted {
    /// Microsoft's sign-in page (opened in the browser).
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AccountLabel {
    pub name: String,
    /// Empty or absent: back to the account's name.
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AccountAddParams {
    pub name: String,
    pub address: String,
    /// Guessed from the address when absent.
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    /// Defaults to the address.
    #[serde(default)]
    pub username: Option<String>,
    /// Guessed from the address when absent.
    #[serde(default)]
    pub smtp_host: Option<String>,
    /// 465 (implicit TLS) or 587 (STARTTLS); 465 when absent.
    #[serde(default)]
    pub smtp_port: Option<u16>,
    /// Goes straight to the system keychain; never stored or logged by Hysj.
    pub password: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SyncParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub account: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SyncResult {
    pub account: String,
    pub new: usize,
}

/// A send-from address belonging to an account.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Identity {
    pub account: String,
    pub address: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ScreenerDecide {
    /// Sender address.
    pub sender: String,
    pub decision: Decision,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ScreenerDecided {
    pub sender: String,
    pub bucket: Bucket,
    /// Messages from this sender, now all in `bucket`.
    pub messages: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ScreenerForget {
    pub sender: String,
}

/// Clear messages from the Desk / Digest. Local to Hysj for now.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct MessageDone {
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SendParams {
    /// One of your identities.
    pub from: String,
    /// Addresses, optionally with names: `Aisha <aisha@example.com>`.
    pub to: Vec<String>,
    #[serde(default)]
    pub cc: Vec<String>,
    pub subject: String,
    pub body: String,
    /// Replying to this message (id from `list`): threading headers are added
    /// and it is cleared from the Desk once sent.
    #[serde(default)]
    pub reply_to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SendResult {
    pub message_id: String,
}

/// Someone you've written to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Contact {
    pub address: String,
    pub name: Option<String>,
    pub times: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct IdentitySuggest {
    /// Recipient address.
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct IdentitySuggestion {
    pub identity: String,
    /// "You last wrote to Aisha from this address" / "Your default for new email".
    pub reason: String,
    /// True when based on your history with this person.
    pub usual: bool,
}

/// Views that aren't classification buckets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum Folder {
    /// Mail you sent (in the synced window).
    Sent,
    /// Mail you cleared, most recently cleared first.
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct FolderParams {
    pub folder: Folder,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub account: Option<String>,
}

/// A one-time code or reset link. `code` is complete only while live;
/// afterwards only its last two digits are kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CodeItem {
    /// Message id.
    pub id: String,
    /// "Sign-in code", "Email verification", "Code", "Password reset".
    pub kind: String,
    pub reset: bool,
    pub service: String,
    pub sender_domain: String,
    pub code: Option<String>,
    pub link: Option<String>,
    pub link_host: Option<String>,
    /// The reset link goes to the sender's own domain.
    pub link_ok: bool,
    /// Unix seconds.
    pub arrived: i64,
    pub expires: i64,
    /// "I didn't ask for this": the link is disabled.
    pub reported: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CodeSettings {
    /// Pop up codes as they arrive.
    pub popup: bool,
    /// Delete expired codes after this many hours (1, 24 or 168).
    pub retention_hours: u32,
}

impl Default for CodeSettings {
    fn default() -> Self {
        Self {
            popup: true,
            retention_hours: 24,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CodeReport {
    pub id: String,
}

/// One booking, merged from its confirmation and later updates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct BookingItem {
    pub key: String,
    /// "flight", "train", "bus", "hotel", "table", "event".
    pub kind: String,
    pub reference: String,
    pub title: String,
    /// ISO 8601 as written in the mail (local time, often without offset).
    pub start: Option<String>,
    pub end: Option<String>,
    /// Unix seconds (approximate when the mail gave no offset): sorting only.
    pub start_ts: Option<i64>,
    pub provider: Option<String>,
    pub city: Option<String>,
    pub seat: Option<String>,
    pub party: Option<u32>,
    /// The start before a later mail moved it.
    pub moved_from: Option<String>,
    /// How many emails were merged into this card.
    pub sources: u32,
    /// The newest source message (id).
    pub message: Option<String>,
    /// A Wallet pass the sender attached (opens in Wallet, which checks it).
    #[serde(default)]
    pub pass: Option<AttachmentParams>,
}

/// One order, merged from confirmation, shipping and delivery mails.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct OrderItem {
    pub key: String,
    pub store: String,
    pub number: String,
    pub ordered: Option<String>,
    pub ordered_ts: Option<i64>,
    pub items: Vec<String>,
    pub total: Option<String>,
    /// 1 Ordered, 2 Shipped, 3 Out for delivery, 4 Delivered.
    pub status: u8,
    pub eta: Option<String>,
    pub delivered_ts: Option<i64>,
    pub tracking_url: Option<String>,
    /// End of the return window (unix seconds), when the store stated one.
    pub return_by: Option<i64>,
    pub sources: u32,
    /// The order confirmation (message id), for "Receipt".
    pub receipt: Option<String>,
}

// --- Showing a message --------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AttachmentInfo {
    /// Position among the message's attachments (for `message/attachment`).
    pub index: u32,
    pub name: String,
    pub mime: String,
    pub size: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct MessageView {
    pub id: String,
    /// Sanitised HTML: no scripts, event handlers, forms or frames; embedded
    /// images inlined as data URIs; remote images left for the reader to allow.
    pub html: String,
    /// The message was plain text (shown in the app's own colours).
    pub plain: bool,
    pub attachments: Vec<AttachmentInfo>,
    /// Remote images in the message.
    pub remote_images: u32,
    /// The reader allowed remote images for this sender.
    pub images_allowed: bool,
    /// Whose images those are (for "Always for …").
    pub sender: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AttachmentParams {
    pub id: String,
    pub index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AttachmentData {
    pub name: String,
    pub mime: String,
    /// Base64.
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SenderImages {
    pub sender: String,
    pub allow: bool,
}

// --- The Digest as an edition -----------------------------------------------------------

/// Which delivery: 0 the latest, 1–2 the two before, 3 everything older.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct EditionParams {
    pub pick: u8,
}

/// One delivery of the Digest, written like a short newspaper: headline and
/// lede, stories in order of what matters, and an ending. Every number in the
/// words comes from the mail itself (templates), never from a model: the
/// headline and lede may be the model's rewording, checked against them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Edition {
    pub pick: u8,
    /// "Wednesday · 16:00 digest · since 08:00 today · 4 min".
    pub meta: String,
    pub headline: String,
    pub lede: String,
    pub stories: Vec<Story>,
    pub ending: Ending,
    /// Every message in this edition (for "Mark as read").
    pub ids: Vec<String>,
    /// Held mail that might matter: asked once per sender.
    pub unsure: Vec<Unsure>,
    /// "Getting too many?" under the FINN story: each one a setting.
    pub controls: Vec<Control>,
}

/// A Digest setting offered in place ("Pause cabin alerts").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Control {
    pub label: String,
    /// `finn_pause:<kind>` or `finn_drops_only`.
    pub key: String,
    /// What pressing it sets the setting to.
    pub on: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AiSettings {
    /// Off: rules only, subject lines instead of gists, nothing downloaded.
    pub enabled: bool,
    /// The model it uses ("gemma3-4b-q4k"); ignored when setting.
    #[serde(default)]
    pub model: String,
    /// Keep writing on battery power (else it waits for the charger).
    #[serde(default)]
    pub on_battery: bool,
    /// The Mac is on battery right now (so the AI is waiting); ignored when setting.
    #[serde(default)]
    pub waiting_for_power: bool,
    /// "standard" (best summaries), "light" (for 8 GB Macs) or "apple".
    #[serde(default)]
    pub size: String,
    /// What this Mac should use (Apple's model if it has it, else by its
    /// memory); ignored when setting.
    #[serde(default)]
    pub recommended: String,
    /// Apple's on-device model can be used here (macOS 26+, Apple Intelligence on).
    #[serde(default)]
    pub apple_available: bool,
    /// Why not ("appleIntelligenceNotEnabled", "deviceNotEligible", "modelNotReady", …).
    #[serde(default)]
    pub apple_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct KindCorrect {
    /// The booking's or order's key.
    pub record: String,
    /// "booking", "order", "subscription", "offer" or "other".
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct DigestHours {
    /// Whole hours, 0–23, one to five of them.
    pub hours: Vec<i8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct DigestSet {
    pub key: String,
    pub on: bool,
}

/// Held mail that can't wait for the digest: one per kind and sender.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Signal {
    /// The newest message (opens in place).
    pub id: String,
    /// Every message in the group (for "Done").
    pub ids: Vec<String>,
    /// "payment", "security" or "review".
    pub kind: String,
    pub from: String,
    pub title: String,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SignalHold {
    pub kind: String,
    pub hold: bool,
}

/// "Not sure about these": held, but from a sender that might matter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Unsure {
    /// The message (opens in place).
    pub id: String,
    pub from: String,
    /// The sender's address (the decision is per sender).
    pub sender: String,
    pub title: String,
    /// "Held because it looks like a notification — but it's from a government sender".
    pub why: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Story {
    /// "listings", "activities", "reading", "social", "services", "money", "other".
    pub kind: String,
    /// The small label above the heading ("Listings").
    pub label: String,
    /// A sentence carrying the facts ("FINN found you 4 jobs and 3 homes.").
    pub head: String,
    pub para: String,
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Section {
    /// "Jobs", "Houses"; none for a story's only section.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub title: Option<String>,
    /// "cards" (photo tiles), "list" (rows), "compact" (thumbnail rows),
    /// "reading" (source, title, gist), "lines" (sender — subject).
    pub layout: String,
    pub items: Vec<Item>,
    /// How many show before "Show N more".
    pub shown: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Item {
    /// The message it came from (opens in place).
    pub id: String,
    pub title: String,
    /// The grey line ("Ski · 5 rom · 168 m²", "FINN · 21 Oct").
    pub sub: String,
    /// Right-aligned mono ("kr 4 250 000", "Frist 1. nov", "08:15").
    pub meta: String,
    /// A one-line gist (reading) or detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub text: Option<String>,
    /// A small chip ("New", "Needs you", "Price dropped").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub tag: Option<String>,
    /// Where the item lives on the web (opened in the browser).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub link: Option<String>,
    /// A remote photo: shown only if the reader allows images from the sender.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub image: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Ending {
    /// "That’s everything since 08:00 today. Next digest 08:00 tomorrow."
    pub text: String,
    /// "12 promotions from 9 shops went straight to the archive."
    pub note: String,
    /// The promotions, folded away.
    pub promotions: Vec<Item>,
}

// --- Searching all mail -------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SearchParams {
    /// Gmail search syntax on Gmail (`from:aisha before:2025/01/01`), words elsewhere.
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AskParams {
    pub question: String,
}

/// What "Ask your mail" found: an answer (checked against its sources) or
/// a note saying why there's none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Asked {
    pub answer: Option<Answer>,
    /// "I couldn't find that in your mail.", or why it couldn't look.
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Answer {
    pub text: String,
    /// "Quoted exactly from the email." / "Summarised from 2 emails — …"
    pub caveat: String,
    pub sources: Vec<Found>,
}

/// A message found on the server. Not kept: its id (`found:<account>|<remote id>`)
/// opens it straight from the server with `message/view`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Found {
    pub id: String,
    pub account: String,
    pub from: String,
    pub from_addr: String,
    pub subject: String,
    /// Unix seconds.
    pub ts: Option<i64>,
}
