use std::future::Future;

use tokio::sync::mpsc;

/// The connection was closed by the other side.
#[derive(Debug, thiserror::Error)]
#[error("transport closed")]
pub struct Closed;

/// A bidirectional stream of JSON-RPC messages (one serialized message each).
/// Implemented in-process by [`ChannelTransport`]; a Unix socket / named pipe
/// implementation arrives with the real daemon process (build step 8).
pub trait Transport: Send {
    fn send(&mut self, msg: String) -> impl Future<Output = Result<(), Closed>> + Send;
    /// `None` once the other side has gone away.
    fn recv(&mut self) -> impl Future<Output = Option<String>> + Send;
}

/// In-process transport over two tokio channels. Used by tests and for running
/// daemon + client in one process during early development.
pub struct ChannelTransport {
    tx: mpsc::UnboundedSender<String>,
    rx: mpsc::UnboundedReceiver<String>,
}

impl ChannelTransport {
    /// Two connected ends: `(client, daemon)`.
    pub fn pair() -> (Self, Self) {
        let (a_tx, a_rx) = mpsc::unbounded_channel();
        let (b_tx, b_rx) = mpsc::unbounded_channel();
        (Self { tx: a_tx, rx: b_rx }, Self { tx: b_tx, rx: a_rx })
    }
}

impl Transport for ChannelTransport {
    async fn send(&mut self, msg: String) -> Result<(), Closed> {
        self.tx.send(msg).map_err(|_| Closed)
    }

    async fn recv(&mut self) -> Option<String> {
        self.rx.recv().await
    }
}
