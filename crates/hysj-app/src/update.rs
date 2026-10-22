//! Updates from GitHub Releases (tauri-plugin-updater; signed with Hysj's
//! updater key, which the plugin checks). Checked on launch and every few
//! hours, downloaded quietly, installed only on restart or quit: replacing
//! the app under a running window would restart its daemon on the new build
//! mid-session. Builds run from `cargo run` never update themselves.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_updater::{Update, UpdaterExt};

/// A downloaded update waiting for a restart.
#[derive(Default)]
pub struct Pending {
    ready: Mutex<Option<(Update, Vec<u8>)>>,
    checked: Mutex<Option<i64>>,
}

#[derive(Clone, Serialize)]
pub struct UpdateStatus {
    pub current: String,
    /// The version downloaded and ready to install.
    pub ready: Option<String>,
    pub notes: Option<String>,
    /// Checks automatically (else only "Check now").
    pub auto: bool,
    /// Unix seconds of the last check this session.
    pub checked: Option<i64>,
    /// Installed from a release (a `cargo run` build can't update itself).
    pub can_update: bool,
}

const EVERY: Duration = Duration::from_secs(6 * 3600);

/// Running from Hysj.app, not a development build.
fn bundled() -> bool {
    std::env::current_exe().is_ok_and(|p| p.to_string_lossy().contains(".app/Contents/MacOS/"))
}

/// The "never check by itself" marker in the app's config folder.
fn auto_off_marker<R: Runtime>(app: &AppHandle<R>) -> Option<std::path::PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|d| d.join("no-auto-update"))
}

pub fn auto<R: Runtime>(app: &AppHandle<R>) -> bool {
    auto_off_marker(app).is_none_or(|p| !p.exists())
}

pub fn set_auto<R: Runtime>(app: &AppHandle<R>, on: bool) -> std::io::Result<()> {
    let Some(p) = auto_off_marker(app) else {
        return Ok(());
    };
    if on {
        match std::fs::remove_file(&p) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    } else {
        std::fs::create_dir_all(p.parent().expect("a config dir"))?;
        std::fs::write(p, b"")
    }
}

pub fn status<R: Runtime>(app: &AppHandle<R>) -> UpdateStatus {
    let pending = app.state::<Pending>();
    let ready = pending.ready.lock().expect("not poisoned");
    UpdateStatus {
        current: app.package_info().version.to_string(),
        ready: ready.as_ref().map(|(u, _)| u.version.clone()),
        notes: ready.as_ref().and_then(|(u, _)| u.body.clone()),
        auto: auto(app),
        checked: *pending.checked.lock().expect("not poisoned"),
        can_update: bundled(),
    }
}

/// Looks for a newer release and downloads it; tells the window when one is ready.
pub async fn check<R: Runtime>(app: &AppHandle<R>) -> Result<UpdateStatus, String> {
    if !bundled() {
        return Ok(status(app));
    }
    let found = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    *app.state::<Pending>().checked.lock().expect("not poisoned") = Some(now_secs());
    if let Some(update) = found {
        let already = status(app).ready.as_deref() == Some(update.version.as_str());
        if !already {
            let bytes = update
                .download(|_, _| {}, || {})
                .await
                .map_err(|e| e.to_string())?;
            tracing::info!(version = %update.version, "update downloaded");
            *app.state::<Pending>().ready.lock().expect("not poisoned") = Some((update, bytes));
            let _ = app.emit("update-ready", ());
        }
    }
    Ok(status(app))
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Checks on launch, then every few hours (spread out, so not every copy of
/// Hysj asks GitHub at the same moment).
pub async fn background<R: Runtime>(app: AppHandle<R>) {
    loop {
        if auto(&app)
            && let Err(e) = check(&app).await
        {
            tracing::warn!(error = %e, "update check failed");
        }
        let jitter = Duration::from_secs((now_secs() as u64 * 7919) % 1800);
        tokio::time::sleep(EVERY + jitter).await;
    }
}

/// Installs a downloaded update (on restart, or when the app quits).
pub fn install<R: Runtime>(app: &AppHandle<R>) -> bool {
    let Some((update, bytes)) = app
        .state::<Pending>()
        .ready
        .lock()
        .expect("not poisoned")
        .take()
    else {
        return false;
    };
    match update.install(bytes) {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!(error = %e, "update install failed");
            false
        }
    }
}
