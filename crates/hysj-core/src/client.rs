use hysj_proto::{Hello, HelloResult, PROTOCOL_VERSION, Request, Response, RpcError, method};
use serde::{Serialize, de::DeserializeOwned};

use crate::transport::{Closed, Transport};

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error(transparent)]
    Closed(#[from] Closed),
    #[error(transparent)]
    Rpc(#[from] RpcError),
    #[error("bad message from daemon: {0}")]
    Json(#[from] serde_json::Error),
}

/// JSON-RPC client over any [`Transport`]. One call in flight at a time.
pub struct Client<T> {
    transport: T,
    next_id: u64,
}

impl<T: Transport> Client<T> {
    /// Connects and performs the version handshake.
    pub async fn connect(transport: T) -> Result<(Self, HelloResult), ClientError> {
        Self::connect_as(transport, None).await
    }

    /// Like [`Self::connect`], telling the daemon what kind of client this is ("app", "cli").
    pub async fn connect_as(
        transport: T,
        client: Option<&str>,
    ) -> Result<(Self, HelloResult), ClientError> {
        let mut c = Self {
            transport,
            next_id: 1,
        };
        let hello = Hello {
            version: PROTOCOL_VERSION,
            client: client.map(str::to_string),
        };
        let result = c.call(method::HELLO, hello).await?;
        Ok((c, result))
    }

    /// The next state-changed push from the daemon (skips anything else).
    /// Use on a connection that makes no calls.
    pub async fn next_event(&mut self) -> Result<hysj_proto::StateChanged, ClientError> {
        loop {
            let raw = self.transport.recv().await.ok_or(Closed)?;
            if let Ok(n) = serde_json::from_str::<hysj_proto::Notification>(&raw) {
                return Ok(n.params);
            }
        }
    }

    pub async fn call<R: DeserializeOwned>(
        &mut self,
        method: &str,
        params: impl Serialize,
    ) -> Result<R, ClientError> {
        let id = self.next_id;
        self.next_id += 1;
        let req = Request::new(id, method, serde_json::to_value(params)?);
        self.transport.send(serde_json::to_string(&req)?).await?;

        loop {
            let raw = self.transport.recv().await.ok_or(Closed)?;
            let resp: Response = serde_json::from_str(&raw)?;
            // ponytail: notifications are dropped until a client needs them (Desk live updates).
            let ours = resp.id == Some(id) || (resp.id.is_none() && resp.error.is_some());
            if !ours {
                continue;
            }
            if let Some(err) = resp.error {
                return Err(err.into());
            }
            return Ok(serde_json::from_value(resp.result.unwrap_or_default())?);
        }
    }
}
