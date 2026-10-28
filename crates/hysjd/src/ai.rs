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
pub const STANDARD: Choice = Choice::Gemma3_4b;
/// For Macs with 8 GB: a quarter of the memory, about six times faster,
/// plainer summaries (measured on real mail).
pub const LIGHT: Choice = Choice::Qwen3_1_7b;

/// This Mac's memory in GB (0 if it can't tell).
pub fn memory_gb() -> u64 {
    std::process::Command::new("sysctl")
        .args(["-n", "hw.memsize"])
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .parse::<u64>()
                .ok()
        })
        .map_or(0, |b| b / (1 << 30))
}

/// "light" on Macs with 8 GB or less, else "standard".
/// Whether Apple's on-device model can be used, as last checked (the
/// helper is asked at start and whenever Settings looks).
static APPLE: std::sync::RwLock<Option<hysj_ai::apple::Availability>> =
    std::sync::RwLock::new(None);

/// Asks `hysj-apple-ai` again; returns the answer.
pub async fn refresh_apple() -> hysj_ai::apple::Availability {
    let a = hysj_ai::apple::availability().await;
    *APPLE.write().expect("not poisoned") = Some(a.clone());
    a
}

pub fn apple() -> hysj_ai::apple::Availability {
    APPLE
        .read()
        .expect("not poisoned")
        .clone()
        .unwrap_or_default()
}

/// Apple's model where macOS has it (nothing to download, about as fast as
/// Light, close to Standard on real mail); else "light" on Macs with 8 GB
/// or less, else "standard".
pub fn recommended_size() -> &'static str {
    if apple().available {
        return "apple";
    }
    match memory_gb() {
        1..=8 => "light",
        _ => "standard",
    }
}

/// The model the reader chose, else the one recommended for this Mac.
/// Apple's, if chosen but no longer available (Apple Intelligence turned
/// off), gives way to the one recommended.
pub fn chosen(store: &hysj_core::Store) -> Choice {
    let size = store.ai_size().ok().flatten();
    let size = match size.as_deref() {
        Some("apple") if !apple().available => None,
        s => s,
    };
    match size.unwrap_or_else(|| recommended_size()) {
        "apple" => Choice::Apple,
        "light" => LIGHT,
        _ => STANDARD,
    }
}

/// "standard", "light" or "apple" for a choice.
pub fn size_of(choice: Choice) -> &'static str {
    match choice {
        Choice::Apple => "apple",
        c if c == LIGHT => "light",
        _ => "standard",
    }
}
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
    let mut jobs = (hub.daemon.lock().await.store)
        .ai_jobs(model, delivery, BATCH)
        .unwrap_or_default();
    let missing: Vec<String> = jobs
        .iter()
        .filter(|j| j.body.is_none())
        .map(|j| j.id.clone())
        .collect();
    hub.fetch_bodies(&missing).await;
    {
        let d = hub.daemon.lock().await;
        for j in jobs.iter_mut().filter(|j| j.body.is_none()) {
            j.body = d.store.get(&j.id).ok().flatten().and_then(|s| s.body);
        }
    }

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
                    // The light model flags too many real newsletters as
                    // offers (which then vanish into promotions): Standard only.
                    offer = i.offer && model == STANDARD.version();
                    if accept_issue(&i.gist, &j.subject) {
                        Ok(capitalized(&i.gist))
                    } else {
                        Err("rejected".to_string())
                    }
                }
                Err(e) => Err(e.to_string()),
            },
            AiTask::Gist => match w.gist(&text).await {
                Ok(g) if accept_gist(&g.gist, &j.subject) => Ok(capitalized(&g.gist)),
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
    let mut c = (hub.daemon.lock().await.store)
        .booking_candidates(model, BATCH)
        .unwrap_or_default();
    let missing: Vec<String> = c
        .iter()
        .filter(|c| c.body.is_none())
        .map(|c| c.id.clone())
        .collect();
    hub.fetch_bodies(&missing).await;
    let candidates = {
        let d = hub.daemon.lock().await;
        for c in c.iter_mut().filter(|c| c.body.is_none()) {
            c.body = d.store.get(&c.id).ok().flatten().and_then(|s| s.body);
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
                // The classifier decided what it is; the model only found the facts.
                let what = c.kind.as_str();
                let mut d = hub.daemon.lock().await;
                // A mail with a booking's number belongs to it: a confirmation
                // without dates, or its payment.
                let linked = || {
                    stated_number(&all, &g.reference)
                        .is_some_and(|n| d.store.link_booking(n, &c.key, date).unwrap_or(false))
                };
                let booking = (what == "booking")
                    .then(|| checked_booking(&all, c.sent, &read))
                    .flatten();
                if let Some(b) = booking {
                    let _ = d.store.record_booking(&c.key, &b, date);
                    Ok("booking".to_string())
                } else if linked() {
                    Ok(format!("linked:{what}"))
                } else if let Some(o) = (what != "booking")
                    .then(|| checked_order(&all, &read))
                    .flatten()
                {
                    let _ = d.store.record_order(&c.key, &o, date);
                    Ok(format!("order:{what}"))
                } else {
                    Ok(format!("unverified:{what}"))
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

/// A sentence starts with a capital ("you must…" → "You must…").
fn capitalized(s: &str) -> String {
    let s = s.trim();
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

/// The Mac is running on battery (`pmset`); false when it can't tell.
/// ponytail: asks pmset every 30 s; IOKit power notifications if that matters.
pub async fn on_battery_power() -> bool {
    tokio::process::Command::new("pmset")
        .args(["-g", "batt"])
        .output()
        .await
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("'Battery Power'"))
}

/// The loaded model, shared by the background passes and Ask your mail
/// (one copy in memory). Apart from the daemon's lock: inference is slow.
#[derive(Default)]
pub struct Slot {
    pub llm: Option<Llm>,
    /// Last used (by a pass or a question), for unloading when idle.
    pub used: Option<std::time::Instant>,
}

impl Slot {
    /// The model `choice`, loaded if it isn't.
    pub async fn get(&mut self, choice: Choice) -> anyhow::Result<&Llm> {
        if self.llm.as_ref().is_some_and(|l| l.choice != choice) {
            self.llm = None;
        }
        if self.llm.is_none() {
            tracing::info!(
                model = choice.version(),
                "loading the local model (downloads once on first use)"
            );
            self.llm = Some(Llm::load(choice).await?);
        }
        self.used = Some(std::time::Instant::now());
        Ok(self.llm.as_ref().expect("loaded above"))
    }
}

pub async fn run(hub: Arc<Hub>) {
    // Offline runs (screenshots, store copies) never load a model.
    if std::env::var_os("HYSJ_NO_SYNC").is_some() || std::env::var_os("HYSJ_NO_AI").is_some() {
        return;
    }
    let mut sorter: Option<crate::kinds::Sorter> = None;
    let mut waiting_for_power = false;
    refresh_apple().await;
    loop {
        {
            let mut slot = hub.llm.lock().await;
            if slot.llm.is_some() && slot.used.is_some_and(|u| u.elapsed() > IDLE) {
                tracing::info!("model idle; unloading");
                slot.llm = None;
            }
        }
        // Turned off in Settings: rules only, and the memory back. On
        // battery it waits for the charger unless the reader said otherwise
        // (a fanless Air gets hot and drains fast while it writes).
        let (enabled, on_battery, choice) = {
            let d = hub.daemon.lock().await;
            (
                d.store.ai_enabled().unwrap_or(true),
                d.store.ai_on_battery().unwrap_or(false),
                chosen(&d.store),
            )
        };
        let model = choice.version();
        // Every model, Apple's too, and every kind of summary waits here.
        let paused = !on_battery && on_battery_power().await;
        if paused != waiting_for_power {
            waiting_for_power = paused;
            tracing::info!(
                paused,
                "{}",
                if paused {
                    "on battery: summaries wait for the charger"
                } else {
                    "on power: summaries resume"
                }
            );
        }
        if !enabled || paused {
            hub.llm.lock().await.llm = None;
            tokio::time::sleep(EVERY).await;
            continue;
        }
        // What kind each mail is comes first: the booking pass reads it.
        if sorter.is_none() {
            match crate::kinds::Sorter::start().await {
                Ok(s) => sorter = Some(s),
                Err(e) => tracing::warn!(error = %e, "can't sort mail by kind; trying again later"),
            }
        }
        let mut sorted = 0;
        if let Some(s) = &mut sorter {
            match s.pass(&hub).await {
                Ok(n) => sorted = n,
                Err(e) => {
                    tracing::warn!(error = %e, "sorting by kind failed");
                    sorter = None;
                }
            }
        }
        if has_work(&hub, model).await {
            // ponytail: held for the whole pass (BATCH mails), so a question
            // can wait one pass; lock per answer if that's felt.
            let mut slot = hub.llm.lock().await;
            let loaded = match slot.get(choice).await {
                Ok(l) => Some(l),
                Err(e) => {
                    tracing::warn!(error = %e, "model unavailable; trying again later");
                    None
                }
            };
            if let Some(l) = loaded {
                let n = pass(&hub, l, model).await;
                drop(slot);
                tracing::info!(stored = n, "model pass");
                if n > 0 {
                    continue; // more work may be waiting
                }
            }
        }
        if sorted > 0 {
            continue;
        }
        tokio::time::sleep(EVERY).await;
    }
}

/// Ask your mail: the passages that may hold the answer, the model's answer
/// from them, and the checks that keep a made-up number or quote from
/// being shown. Never under the daemon's lock while the model works.
pub async fn ask(hub: &Hub, question: &str) -> hysj_proto::Asked {
    use hysj_core::ask;
    let none = |note: &str| hysj_proto::Asked {
        answer: None,
        note: note.into(),
    };
    // Asked before the first check (just started, or offline): ask now, or
    // the choice falls back to a model that may need downloading.
    if APPLE.read().expect("not poisoned").is_none() {
        refresh_apple().await;
    }
    let (enabled, choice, docs) = {
        let d = hub.daemon.lock().await;
        (
            d.store.ai_enabled().unwrap_or(true),
            chosen(&d.store),
            d.store.searchable().unwrap_or_default(),
        )
    };
    if !enabled || std::env::var_os("HYSJ_NO_AI").is_some() {
        return none("Turn on the on-device AI in Settings to ask your mail.");
    }
    let mut slot = hub.llm.lock().await;
    let llm = match slot.get(choice).await {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!(error = %e, "model unavailable for a question");
            return none("The on-device AI isn’t ready yet. Try again in a moment.");
        }
    };
    let mut terms = llm.terms(question).await.unwrap_or_default();
    terms.extend(ask::question_terms(question));
    let passages = ask::retrieve(&docs, &terms);
    let not_found = "I couldn’t find that in your mail.";
    if passages.is_empty() {
        return none(not_found);
    }
    let shown: String = passages
        .iter()
        .enumerate()
        .map(|(i, p)| format!("[{i}] {}\n", ask::shown(p)))
        .collect();
    let guess = match llm.answer(question, &shown).await {
        Ok(g) => g,
        Err(e) => {
            tracing::warn!(error = %e, "no answer from the model");
            return none(not_found);
        }
    };
    drop(slot);
    let checked = ask::check(
        &ask::Guess {
            found: guess.found,
            answer: &guess.answer,
            sources: &guess.sources,
            quote: &guess.quote,
        },
        &passages,
    );
    // Never the question or the answer: the log must not hold mail content.
    tracing::info!(
        passages = passages.len(),
        answered = checked.is_some(),
        "question"
    );
    let Some(c) = checked else {
        return none(not_found);
    };
    let d = hub.daemon.lock().await;
    let sources = c
        .sources
        .iter()
        .filter_map(|&i| d.store.get(&passages[i].id).ok().flatten())
        .map(|s| hysj_proto::Found {
            id: s.summary.id,
            account: s.summary.account,
            from: s.summary.from,
            from_addr: s.summary.from_addr,
            subject: s.summary.subject,
            ts: s.summary.ts,
        })
        .collect();
    hysj_proto::Asked {
        answer: Some(hysj_proto::Answer {
            caveat: ask::caveat(&c),
            text: capitalized(&c.text),
            sources,
        }),
        note: String::new(),
    }
}
