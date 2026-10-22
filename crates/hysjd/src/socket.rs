//! Unix domain socket transport: one JSON message per line, owner-only socket
//! file, no TCP listener (other local users must not read mail).
//! ponytail: macOS/Linux only; a named pipe for Windows when Windows is in scope.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use hysj_core::{Closed, Transport};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::OwnedWriteHalf;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;

use crate::Hub;

/// `$HYSJ_SOCKET`, else next to the store (owner-only directory).
pub fn socket_path() -> std::io::Result<PathBuf> {
    let path: PathBuf = match std::env::var_os("HYSJ_SOCKET") {
        Some(p) => p.into(),
        None => crate::default_db_path()?.with_file_name("hysjd.sock"),
    };
    // sockaddr_un holds ~104 bytes on macOS (108 on Linux).
    if path.as_os_str().len() > 100 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "socket path too long for a Unix socket ({} bytes); set HYSJ_SOCKET to a shorter path",
                path.as_os_str().len()
            ),
        ));
    }
    Ok(path)
}

pub struct SocketTransport {
    rx: mpsc::UnboundedReceiver<String>,
    tx: OwnedWriteHalf,
}

impl SocketTransport {
    pub fn new(stream: UnixStream) -> Self {
        let (read, tx) = stream.into_split();
        let (lines_tx, rx) = mpsc::unbounded_channel();
        // Reading happens in its own task, so `recv` is cancel-safe for select!.
        tokio::spawn(async move {
            let mut lines = BufReader::new(read).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if lines_tx.send(line).is_err() {
                    break;
                }
            }
        });
        Self { rx, tx }
    }
}

impl Transport for SocketTransport {
    async fn send(&mut self, mut msg: String) -> Result<(), Closed> {
        msg.push('\n');
        self.tx.write_all(msg.as_bytes()).await.map_err(|_| Closed)
    }

    async fn recv(&mut self) -> Option<String> {
        self.rx.recv().await
    }
}

pub async fn connect(path: &Path) -> std::io::Result<SocketTransport> {
    Ok(SocketTransport::new(UnixStream::connect(path).await?))
}

/// Binds the socket and serves clients forever. Refuses to start if another
/// daemon answers on it; replaces a stale socket file left by a crash.
pub async fn listen(path: &Path, hub: Arc<Hub>) -> std::io::Result<()> {
    if path.exists() {
        if UnixStream::connect(path).await.is_ok() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                "hysjd is already running",
            ));
        }
        std::fs::remove_file(path)?;
    }
    let listener = UnixListener::bind(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    tracing::info!("listening");
    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(crate::serve(SocketTransport::new(stream), hub.clone()));
    }
}

/// Connects to the running daemon, starting it if needed: this same program,
/// run with `--daemon`, so the daemon is always the current build.
/// ponytail: no launchd/systemd login item yet; the first client starts it.
pub async fn connect_or_start() -> std::io::Result<SocketTransport> {
    let path = socket_path()?;
    if let Ok(t) = connect(&path).await {
        return Ok(t);
    }
    let exe = std::env::current_exe()?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path.with_file_name("hysjd.log"))?;
    std::process::Command::new(&exe)
        .arg(crate::DAEMON_FLAG)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(log)
        .spawn()
        .map_err(|e| std::io::Error::new(e.kind(), format!("starting {}: {e}", exe.display())))?;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Ok(t) = connect(&path).await {
            return Ok(t);
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        "hysjd did not start (see hysjd.log)",
    ))
}

/// A connected, greeted client for the running daemon. If an older daemon
/// is running it exits on our hello; we retry until the current one is up.
pub async fn client(kind: Option<&str>) -> std::io::Result<hysj_core::Client<SocketTransport>> {
    use hysj_core::ClientError;
    let mut last = None;
    // Up to ~75 s: a daemon from before this logic exits only when its own
    // binary changes, checked once a minute.
    for _ in 0..300 {
        let transport = connect_or_start().await?;
        match hysj_core::Client::connect_as(transport, kind).await {
            Ok((c, _)) => return Ok(c),
            Err(ClientError::Rpc(e)) if e.code == hysj_proto::RpcError::VERSION_MISMATCH => {
                last = Some(e.message)
            }
            Err(ClientError::Closed(_)) => last = Some("daemon restarting".into()),
            Err(e) => return Err(std::io::Error::other(e.to_string())),
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Err(std::io::Error::other(
        last.unwrap_or_else(|| "could not reach the daemon".into()),
    ))
}
