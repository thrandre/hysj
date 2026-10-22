//! The local model at work: gists, suggested replies, Digest headlines,
//! bookings and senders, written in the background for what reaches the user. Inference never runs under
//! the hub lock: each pass locks briefly to pick jobs (and fetch their bodies),
//! infers unlocked, then locks again to store results.
//!
//! The model (~4 GB) is loaded when there's work and dropped after `IDLE`
//! without any. ponytail: runs on battery too; check power state if it matters.

use std::sync::Arc;
use std::time::Duration;

use hysj_ai::{Choice, Llm, MessageText, Writer, accept_gist, accept_issue, accept_replies};
use hysj_core::digest::last_slot;
use hysj_core::store::AiTask;

use crate::Hub;

/// The model Hysj uses (chosen by benchmark: Gemma 3 4B).
pub const MODEL: Choice = Choice::Gemma3_4b;
/// Messages per pass, so the lock is released often.
pub const BATCH: usize = 4;
const EVERY: Duration = Duration::from_secs(30);
/// Unload the model after this long without work.
const IDLE: Duration = Duration::from_secs(10 * 60);

/// The latest Digest delivery as `(after, upto]`, unix seconds.
fn latest_delivery() -> (i64, i64) {
    let now = jiff::Zoned::now();
    let slot = last_slot(&now);
    let before = last_slot(
        &slot
            .checked_sub(jiff::SignedDuration::from_secs(1))
            .expect("valid time"),
    );
    (before.timestamp().as_second(), slot.timestamp().as_second())
}

/// One round of work. Returns how many answers were stored.
pub async fn pass<W: Writer>(hub: &Hub, w: &W, model: &str) -> usize {
    let delivery = latest_delivery();
    // Pick jobs and make sure their bodies are here (fetched like `show`).
    let jobs = {
        let mut d = hub.daemon.lock().await;
        let mut jobs = d.store.ai_jobs(model, delivery, BATCH).unwrap_or_default();
        for j in jobs.iter_mut().filter(|j| j.body.is_none()) {
            if let Ok(m) = d.show(&j.id).await {
                j.body = Some(m.body);
            }
        }
        jobs
    };

    let mut stored = 0;
    for j in jobs {
        let Some(body) = j.body.as_deref() else {
            continue;
        };
        let text = MessageText {
            from: &j.from,
            subject: &j.subject,
            body,
        };
        let mut offer = false;
        let result = match j.task {
            AiTask::Issue => match w.issue(&text).await {
                Ok(i) => {
                    offer = i.offer;
                    if accept_issue(&i.gist, &j.subject) {
                        Ok(i.gist)
                    } else {
                        Err("rejected".to_string())
                    }
                }
                Err(e) => Err(e.to_string()),
            },
            AiTask::Gist => match w.gist(&text).await {
                Ok(g) if accept_gist(&g.gist, &j.subject) => Ok(g.gist),
                Ok(_) => Err("rejected".to_string()),
                Err(e) => Err(e.to_string()),
            },
            AiTask::Replies => match w.replies(&text).await {
                Ok(r) => {
                    let ok = accept_replies(&r.replies);
                    if ok.is_empty() {
                        Err("none".to_string())
                    } else {
                        Ok(serde_json::to_string(&ok).expect("strings"))
                    }
                }
                Err(e) => Err(e.to_string()),
            },
        };
        if let Err(e) = &result {
            // Never the text: the log must not hold mail content.
            tracing::debug!(task = j.task.name(), error = %e, "model answer not used");
        }
        let d = hub.daemon.lock().await;
        if offer {
            let _ = d.store.ai_store(&j.key, model, "offer", Ok("1".into()));
        }
        if d.store
            .ai_store(&j.key, model, j.task.name(), result)
            .is_ok()
        {
            stored += 1;
        }
    }

    // The Digest's headline and lede, written ahead for the next edition.
    // Every number must come from the templated facts, or the templates stand.
    let editions = hub.daemon.lock().await.digest_to_write(model);
    for (key, facts) in editions {
        let result = match w.digest(&facts).await {
            Ok(t)
                if hysj_core::edition::model_headline_ok(&t.headline)
                    && hysj_core::edition::model_text_ok(
                        &facts,
                        &format!("{} {}", t.headline, t.summary),
                    ) =>
            {
                Ok(serde_json::to_string(&(t.headline.trim(), t.summary.trim())).expect("strings"))
            }
            Ok(_) => Err("numbers not in the facts".to_string()),
            Err(e) => Err(e.to_string()),
        };
        let d = hub.daemon.lock().await;
        if d.store.ai_store(&key, model, "digest", result).is_ok() {
            stored += 1;
        }
    }
    // Bookings and orders without schema.org data: the model says what the
    // mail is and where the facts are; only a number and dates the mail
    // itself states are kept.
    let candidates = {
        let mut d = hub.daemon.lock().await;
        let mut c = d.store.booking_candidates(model, BATCH).unwrap_or_default();
        for c in c.iter_mut().filter(|c| c.body.is_none()) {
            if let Ok(m) = d.show(&c.id).await {
                c.body = Some(m.body);
            }
        }
        c.into_iter()
            .map(|c| {
                let history = d.store.sender_history(&c.addr, &c.key).unwrap_or_default();
                (c, history)
            })
            .collect::<Vec<_>>()
    };
    for (c, history) in candidates {
        let Some(body) = c.body.as_deref() else {
            continue;
        };
        let text = MessageText {
            from: &c.from,
            subject: &c.subject,
            body,
        };
        let all = format!("{}\n{body}", c.subject);
        let date = c.sent.unwrap_or_else(|| jiff::Timestamp::now().as_second());
        let result = match w.booking(&text, &history).await {
            Ok(g) => {
                use hysj_core::extract::{
                    ModelRead, checked_booking, checked_order, stated_number,
                };
                let read = ModelRead {
                    kind: &g.kind,
                    title: &g.title,
                    reference: &g.reference,
                    start: &g.start,
                    end: &g.end,
                    provider: &g.provider,
                    city: &g.city,
                    total: &g.total,
                    ships: g.ships,
                };
                let what = g.what.as_str();
                let order = matches!(what, "purchase" | "rental" | "subscription");
                let mut d = hub.daemon.lock().await;
                // Decided by a fact, not the label: a stated time to show up,
                // after the mail was sent, is a booking (a paid ticket too).
                let booking = if what == "booking" {
                    checked_booking(&all, c.sent, &read)
                } else if !g.attend.trim().is_empty() {
                    // "16:00" alone takes its date from the start.
                    let attend = g.attend.trim();
                    let at = if attend.len() >= 10 {
                        attend.to_string()
                    } else {
                        format!("{}T{attend}", g.start.get(..10).unwrap_or_default())
                    };
                    checked_booking(&all, c.sent, &ModelRead { start: &at, ..read })
                } else {
                    None
                };
                // A mail with a booking's number belongs to it: a confirmation
                // without dates, or its payment.
                let linked = || {
                    stated_number(&all, &g.reference)
                        .is_some_and(|n| d.store.link_booking(n, &c.key, date).unwrap_or(false))
                };
                if let Some(b) = booking {
                    let _ = d.store.record_booking(&c.key, &b, date);
                    Ok("booking".to_string())
                } else if linked() {
                    Ok(format!("linked:{what}"))
                } else if let Some(o) = order.then(|| checked_order(&all, &read)).flatten() {
                    let _ = d.store.record_order(&c.key, &o, date);
                    Ok(format!("order:{what}"))
                } else if order || what == "booking" {
                    Ok(format!("unverified:{what}"))
                } else {
                    Ok(format!("none:{what}"))
                }
            }
            Err(e) => Err(e.to_string()),
        };
        let d = hub.daemon.lock().await;
        if d.store.ai_store(&c.key, model, "booking", result).is_ok() {
            stored += 1;
        }
    }

    // Bulk senders: shop or publication (once per sender; the Digest files a
    // shop's mail as promotions).
    let senders = {
        let d = hub.daemon.lock().await;
        d.store.senders_to_sort(model, BATCH).unwrap_or_default()
    };
    for (address, name, subjects) in senders {
        let result = match w.sender_kind(&name, &subjects).await {
            Ok(k) if k.kind == "shop" || k.kind == "publication" => Ok(k.kind),
            Ok(_) => Err("unknown kind".to_string()),
            Err(e) => Err(e.to_string()),
        };
        let d = hub.daemon.lock().await;
        if d.store
            .ai_store(&format!("sender:{address}"), model, "sender_kind", result)
            .is_ok()
        {
            stored += 1;
        }
    }
    if stored > 0 {
        hub.changed();
    }
    stored
}

/// Whether there is anything for the model to do (so it isn't loaded for nothing).
async fn has_work(hub: &Hub, model: &str) -> bool {
    let delivery = latest_delivery();
    let d = hub.daemon.lock().await;
    let jobs = d
        .store
        .ai_jobs(model, delivery, 1)
        .map(|j| !j.is_empty())
        .unwrap_or(false);
    let senders = d
        .store
        .senders_to_sort(model, 1)
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    let bookings = d
        .store
        .booking_candidates(model, 1)
        .map(|b| !b.is_empty())
        .unwrap_or(false);
    jobs || senders || bookings || !d.digest_to_write(model).is_empty()
}

pub async fn run(hub: Arc<Hub>) {
    // Offline runs (screenshots, store copies) never load a model.
    if std::env::var_os("HYSJ_NO_SYNC").is_some() || std::env::var_os("HYSJ_NO_AI").is_some() {
        return;
    }
    let model = MODEL.version();
    let mut llm: Option<Llm> = None;
    let mut last_work = std::time::Instant::now();
    loop {
        if llm.is_some() && last_work.elapsed() > IDLE {
            tracing::info!("model idle; unloading");
            llm = None;
        }
        // Turned off in Settings: rules only, and the memory back.
        if !hub.daemon.lock().await.store.ai_enabled().unwrap_or(true) {
            llm = None;
            tokio::time::sleep(EVERY).await;
            continue;
        }
        if has_work(&hub, model).await {
            last_work = std::time::Instant::now();
            if llm.is_none() {
                tracing::info!(
                    model,
                    "loading the local model (downloads once on first use)"
                );
                match Llm::load(MODEL).await {
                    Ok(l) => llm = Some(l),
                    Err(e) => tracing::warn!(error = %e, "model unavailable; trying again later"),
                }
            }
            if let Some(l) = &llm {
                let n = pass(&hub, l, model).await;
                tracing::info!(stored = n, "model pass");
                if n > 0 {
                    continue; // more work may be waiting
                }
            }
        }
        tokio::time::sleep(EVERY).await;
    }
}
