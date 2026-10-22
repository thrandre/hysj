//! Background work: sync every minute, announce Digest deliveries, and post OS
//! notifications for codes when no app window is open.
//! ponytail: polling; IMAP IDLE (push) per account would make codes instant.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use hysj_core::digest;
use hysj_proto::Bucket;

use crate::Hub;

pub const SYNC_EVERY: Duration = Duration::from_secs(60);

/// A rebuilt hysjd replaces this one: clients restart it on reconnect. Its own
/// task, so a crashed sync can't keep an outdated daemon alive.
pub async fn watch_exe() {
    let exe_time = || {
        std::env::current_exe()
            .and_then(std::fs::metadata)
            .and_then(|m| m.modified())
            .ok()
    };
    let Some(started_as) = exe_time() else { return };
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        if exe_time().is_some_and(|t| t != started_as) {
            tracing::info!("binary changed on disk; exiting so the new one takes over");
            std::process::exit(0);
        }
    }
}

/// Runs a background task, restarting it (after a pause) if it panics.
pub async fn supervise<F, Fut>(name: &'static str, task: F)
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    loop {
        match tokio::spawn(task()).await {
            Err(e) if e.is_panic() => {
                tracing::error!(task = name, "crashed; restarting in a minute")
            }
            _ => return,
        }
        tokio::time::sleep(SYNC_EVERY).await;
    }
}

pub async fn run(hub: Arc<Hub>) {
    let mut tick: u64 = 0;
    let now = jiff::Zoned::now();
    let mut announced = digest::last_slot(&now);
    // Codes already there at start are old news.
    let mut notified: HashSet<String> = {
        let d = hub.daemon.lock().await;
        d.store
            .codes(now.timestamp().as_second())
            .map(|c| c.into_iter().map(|c| c.id).collect())
            .unwrap_or_default()
    };
    loop {
        tick += 1;
        // Passwords first, without the lock: after a rebuild macOS asks
        // whether this binary may read the keychain, and the app must keep
        // working while that dialog waits for an answer.
        if std::env::var_os("HYSJ_NO_SYNC").is_none() {
            let missing = hub.daemon.lock().await.passwords_missing();
            for address in missing {
                let a = address.clone();
                match tokio::task::spawn_blocking(move || crate::password(&a)).await {
                    Ok(Ok(p)) => {
                        hub.daemon.lock().await.remember_password(address, p);
                        // Allowed at last: sync again without a manual Sync.
                        hub.sync_paused
                            .store(false, std::sync::atomic::Ordering::Relaxed);
                    }
                    Ok(Err(e)) => tracing::warn!(error = %e, "no password for an account yet"),
                    Err(e) => tracing::warn!(error = %e, "keychain read failed"),
                }
            }
        }
        let (codes, delivered, new_mail) = {
            let mut d = hub.daemon.lock().await;
            // HYSJ_NO_SYNC: offline runs (screenshots, tests on a store copy).
            let offline = std::env::var_os("HYSJ_NO_SYNC").is_some();
            let mut new_mail = false;
            if !offline && !hub.sync_paused.load(std::sync::atomic::Ordering::Relaxed) {
                match d.sync(None).await {
                    Ok(r) => new_mail = r.iter().any(|r| r.new > 0),
                    Err(e) if e.message.starts_with("keychain") => {
                        hub.sync_paused
                            .store(true, std::sync::atomic::Ordering::Relaxed);
                        tracing::warn!(error = %e, "keychain unavailable; scheduled sync paused until a manual Sync");
                    }
                    Err(e) => tracing::warn!(error = %e, "scheduled sync failed"),
                }
            }
            let now = jiff::Timestamp::now().as_second();
            let codes = d.store.codes(now).unwrap_or_default();
            let slot = digest::last_slot(&jiff::Zoned::now());
            let delivered = if slot > announced {
                let from = announced.timestamp().as_second();
                let to = slot.timestamp().as_second();
                let n = d
                    .store
                    .list(Bucket::Digest, None)
                    .map(|l| {
                        l.iter()
                            .filter(|m| m.ts.is_some_and(|t| t > from && t <= to))
                            .count()
                    })
                    .unwrap_or(0);
                Some((slot.clone(), n))
            } else {
                None
            };
            (codes, delivered, new_mail)
        };

        let now = jiff::Timestamp::now().as_second();
        let mut fresh_code = false;
        for c in codes.iter().filter(|c| c.expires > now) {
            if !notified.insert(c.id.clone()) {
                continue;
            }
            fresh_code = true;
            // The app shows its own pop-up; the OS one is for when it's closed.
            // No digits: Notification Center would keep them past expiry.
            if !hub.app_connected() {
                let mins = (c.expires - now + 59) / 60;
                notify(
                    &format!("{} from {}", c.kind, c.service),
                    &format!("Expires in {mins} min. Open Hysj to copy it."),
                );
            }
        }
        // Tell clients only when something changed (plus a slow heartbeat for
        // label changes and deletions, which sync doesn't count).
        if new_mail || fresh_code || delivered.is_some() || tick.is_multiple_of(10) {
            hub.changed();
        }
        if let Some((slot, n)) = delivered {
            announced = slot.clone();
            if n > 0 {
                notify(
                    &format!(
                        "Your {} digest is ready",
                        digest::name(&slot).to_lowercase()
                    ),
                    &format!("{n} newsletters, notifications and receipts"),
                );
            }
        }
        tokio::time::sleep(SYNC_EVERY).await;
    }
}

/// A desktop notification. Best effort: failures are only logged.
fn notify(title: &str, body: &str) {
    let quote = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let result = if cfg!(target_os = "macos") {
        let script = format!(
            "display notification \"{}\" with title \"Hysj\" subtitle \"{}\"",
            quote(body),
            quote(title)
        );
        tokio::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .spawn()
    } else {
        tokio::process::Command::new("notify-send")
            .arg(format!("Hysj: {title}"))
            .arg(body)
            .spawn()
    };
    if let Err(e) = result {
        tracing::warn!(error = %e, "notification failed");
    }
}
