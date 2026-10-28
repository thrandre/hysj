//! Models, store, sync logic and client plumbing shared by the daemon and its
//! clients. No UI and no network-library types in here.

pub mod ask;
pub mod backend;
pub mod classify;
pub mod client;
pub mod digest;
pub mod edition;
pub mod extract;
pub mod finn;
pub mod kinds;
pub mod store;
pub mod sync;
pub mod text;
pub mod transport;
pub mod view;

pub use backend::{BackendError, Changes, MailBackend, Outgoing, RawHeaders, SENT_TAG};
pub use client::{Client, ClientError};
pub use store::{Store, StoreError, Stored};
pub use transport::{ChannelTransport, Closed, Transport};
