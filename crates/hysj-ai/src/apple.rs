//! Apple's on-device model (Foundation Models, macOS 26+ with Apple
//! Intelligence) through `hysj-apple-ai`, a small Swift helper that ships
//! next to Hysj's binary: one JSON request per line on its stdin, one answer
//! per line on its stdout. Nothing to download; macOS runs the model.

use std::path::PathBuf;
use std::process::Stdio;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

/// `$HYSJ_APPLE_HELPER`, else `hysj-apple-ai` beside the running binary
/// (Hysj.app/Contents/MacOS, or target/<profile> when built from source).
pub fn helper_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("HYSJ_APPLE_HELPER") {
        return Some(p.into());
    }
    let p = std::env::current_exe()
        .ok()?
        .parent()?
        .join("hysj-apple-ai");
    p.exists().then_some(p)
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Availability {
    pub available: bool,
    /// Why not ("appleIntelligenceNotEnabled", "deviceNotEligible", …).
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub languages: Vec<String>,
}

pub struct Helper {
    _child: Child,
    io: Mutex<(ChildStdin, BufReader<ChildStdout>)>,
}

impl Helper {
    pub fn spawn() -> Result<Self> {
        let path = helper_path().context("hysj-apple-ai isn't installed with this build")?;
        let mut child = Command::new(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .context("starting hysj-apple-ai")?;
        let stdin = child.stdin.take().expect("piped");
        let stdout = BufReader::new(child.stdout.take().expect("piped"));
        Ok(Self {
            _child: child,
            io: Mutex::new((stdin, stdout)),
        })
    }

    /// One request, one answer (requests are taken one at a time).
    async fn request(&self, req: &Value) -> Result<Value> {
        let mut io = self.io.lock().await;
        let (stdin, stdout) = &mut *io;
        stdin.write_all(format!("{req}\n").as_bytes()).await?;
        stdin.flush().await?;
        let mut line = String::new();
        if stdout.read_line(&mut line).await? == 0 {
            bail!("hysj-apple-ai stopped");
        }
        Ok(serde_json::from_str(&line)?)
    }

    pub async fn check(&self) -> Result<Availability> {
        Ok(serde_json::from_value(
            self.request(&json!({"check": true})).await?,
        )?)
    }

    /// The model's answer to `user` under `system`, fitted to `schema`, as JSON.
    pub async fn ask(
        &self,
        system: &str,
        user: &str,
        schema: &Value,
        max_tokens: usize,
        temperature: f64,
    ) -> Result<String> {
        let a = self
            .request(&json!({
                "system": system, "user": user, "schema": schema,
                "max_tokens": max_tokens, "temperature": temperature,
            }))
            .await?;
        match (a["ok"].as_str(), a["error"].as_str()) {
            (Some(ok), _) => Ok(ok.to_string()),
            (_, Some(e)) => bail!("Apple's model: {e}"),
            _ => bail!("hysj-apple-ai answered nothing"),
        }
    }
}

/// Whether Apple's model can be used on this Mac right now.
pub async fn availability() -> Availability {
    let Ok(h) = Helper::spawn() else {
        return Availability {
            reason: "not installed with this build".into(),
            ..Default::default()
        };
    };
    h.check().await.unwrap_or_else(|e| Availability {
        reason: e.to_string(),
        ..Default::default()
    })
}
