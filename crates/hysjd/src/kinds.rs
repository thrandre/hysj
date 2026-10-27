//! Sorting digest mail by kind (booking, order, subscription, offer, other)
//! with `hysj_core::kinds`, over Apple's sentence vectors (`hysj-apple-ai`,
//! NaturalLanguage: no Apple Intelligence or download needed).
//!
//! Two passes: subject and sender for every mail (no body needed), then the
//! full text for the few that might be a booking or an order. The result is
//! stored as the `kind` task; the model's booking pass reads only what's
//! sure to be a booking, order or subscription.

use std::hash::{Hash, Hasher};

use anyhow::Result;
use hysj_ai::apple::Helper;
use hysj_core::kinds::{self, Classifier, KINDS, MAYBE, SURE};

use crate::Hub;

/// Mails sorted per pass, so the lock is released often.
const BATCH: usize = 16;
/// A correction counts this many invented examples.
const CORRECTION: f32 = 5.0;

/// The corrections trained with, then the first and second pass.
type Trained = (Vec<(String, String)>, Classifier, Classifier);

pub struct Sorter {
    helper: Helper,
    /// The embedding model's id: vectors from different models never mix.
    model: String,
    /// The corrections it was trained with, and the two passes' classifiers.
    trained: Option<Trained>,
}

fn hashed(prefix: &str, text: &str) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    format!("{prefix}:{:016x}", h.finish())
}

impl Sorter {
    pub async fn start() -> Result<Self> {
        let helper = Helper::spawn()?;
        let (model, _) = helper.embed(&["Hysj".to_string()]).await?;
        Ok(Self {
            helper,
            model,
            trained: None,
        })
    }

    /// Vectors for `keyed` texts, from the store or embedded (and stored).
    async fn vectors(&self, hub: &Hub, keyed: &[(String, String)]) -> Result<Vec<Vec<f32>>> {
        let mut out: Vec<Option<Vec<f32>>> = {
            let d = hub.daemon.lock().await;
            keyed
                .iter()
                .map(|(k, _)| d.store.vector(k, &self.model).ok().flatten())
                .collect()
        };
        let missing: Vec<usize> = (0..keyed.len()).filter(|i| out[*i].is_none()).collect();
        for chunk in missing.chunks(32) {
            let texts: Vec<String> = chunk.iter().map(|i| keyed[*i].1.clone()).collect();
            let (_, vs) = self.helper.embed(&texts).await?;
            let d = hub.daemon.lock().await;
            for (i, v) in chunk.iter().zip(vs) {
                let _ = d.store.put_vector(&keyed[*i].0, &self.model, &v);
                out[*i] = Some(v);
            }
        }
        Ok(out.into_iter().map(|v| v.unwrap_or_default()).collect())
    }

    /// Trains on the seed set and the reader's corrections, again whenever
    /// those change.
    async fn train(&mut self, hub: &Hub) -> Result<()> {
        let labels = hub.daemon.lock().await.store.kind_labels()?;
        if self.trained.as_ref().is_some_and(|t| t.0 == labels) {
            return Ok(());
        }
        let seed = kinds::seed();
        let full: Vec<(String, String)> = seed
            .iter()
            .map(|(_, t)| (hashed("seed", t), t.clone()))
            .collect();
        let head: Vec<(String, String)> = seed
            .iter()
            .map(|(_, t)| {
                (
                    hashed("seed-head", kinds::head(t)),
                    kinds::head(t).to_string(),
                )
            })
            .collect();
        let full_v = self.vectors(hub, &full).await?;
        let head_v = self.vectors(hub, &head).await?;
        let mut full_ex: Vec<(Vec<f32>, usize, f32)> = full_v
            .into_iter()
            .zip(&seed)
            .map(|(v, (k, _))| (v, *k, 1.0))
            .collect();
        let mut head_ex: Vec<(Vec<f32>, usize, f32)> = head_v
            .into_iter()
            .zip(&seed)
            .map(|(v, (k, _))| (v, *k, 1.0))
            .collect();
        {
            let d = hub.daemon.lock().await;
            for (key, kind) in &labels {
                let Some(k) = kinds::index(kind) else {
                    continue;
                };
                if let Ok(Some(v)) = d.store.vector(key, &self.model) {
                    full_ex.push((v, k, CORRECTION));
                }
                if let Ok(Some(v)) = d.store.vector(&format!("head:{key}"), &self.model) {
                    head_ex.push((v, k, CORRECTION));
                }
            }
        }
        let as_ex = |e: &[(Vec<f32>, usize, f32)]| -> Classifier {
            let ex: Vec<(&[f32], usize, f32)> =
                e.iter().map(|(v, k, w)| (v.as_slice(), *k, *w)).collect();
            Classifier::train(&ex)
        };
        let (h, f) = (as_ex(&head_ex), as_ex(&full_ex));
        tracing::info!(
            examples = full_ex.len(),
            corrections = labels.len(),
            "kind classifier trained"
        );
        self.trained = Some((labels, h, f));
        Ok(())
    }

    /// Sorts the next few mails. Returns how many.
    pub async fn pass(&mut self, hub: &Hub) -> Result<usize> {
        self.train(hub).await?;
        let jobs = hub.daemon.lock().await.store.unsorted(&self.model, BATCH)?;
        if jobs.is_empty() {
            return Ok(0);
        }
        let heads: Vec<(String, String)> = jobs
            .iter()
            .map(|j| {
                (
                    format!("head:{}", j.key),
                    kinds::text(&j.subject, &j.from, None),
                )
            })
            .collect();
        let head_v = self.vectors(hub, &heads).await?;
        let (_, head, full) = self.trained.as_ref().expect("trained above");
        // Only what might be a booking, order or subscription gets read in full.
        // So does all mail from a sender who has sent one before.
        let mut deeper = Vec::new();
        let mut results = Vec::new();
        {
            let d = hub.daemon.lock().await;
            for (j, v) in jobs.iter().zip(&head_v) {
                let p = head.probs(v);
                if p[0] + p[1] + p[2] >= MAYBE || d.store.sender_has_kind(&j.addr).unwrap_or(false)
                {
                    deeper.push(j);
                } else {
                    results.push((j.key.clone(), j.addr.clone(), "skipped".to_string()));
                }
            }
        }
        let ids: Vec<String> = deeper.iter().map(|j| j.id.clone()).collect();
        hub.fetch_bodies(&ids).await;
        let mut texts = Vec::new();
        let mut failed = Vec::new();
        {
            let d = hub.daemon.lock().await;
            for j in &deeper {
                let raw = d.store.raw(&j.id).ok().flatten();
                let body = raw.as_deref().and_then(kinds::reading);
                match body {
                    Some(b) => {
                        texts.push((j.key.clone(), kinds::text(&j.subject, &j.from, Some(&b))))
                    }
                    None => failed.push(j.key.clone()),
                }
            }
        }
        let full_v = self.vectors(hub, &texts).await?;
        let addr = |key: &str| {
            jobs.iter()
                .find(|j| j.key == key)
                .map(|j| j.addr.clone())
                .unwrap_or_default()
        };
        for ((key, _), v) in texts.iter().zip(&full_v) {
            let (k, p) = full.predict(v);
            let kind = KINDS[k];
            tracing::debug!(kind, p, "sorted");
            results.push((
                key.clone(),
                addr(key),
                if p >= SURE {
                    kind.to_string()
                } else {
                    format!("unsure:{kind}")
                },
            ));
        }
        let d = hub.daemon.lock().await;
        for (key, addr, r) in &results {
            let _ = d.store.ai_store(key, &self.model, "kind", Ok(r.clone()));
            if matches!(r.as_str(), "booking" | "order" | "subscription") {
                let _ = d.store.resort_sender(addr);
            }
        }
        // Tried again later (twice at most), like the model's tasks.
        for key in &failed {
            let _ = d
                .store
                .ai_store(key, &self.model, "kind", Err("no body".into()));
        }
        Ok(jobs.len())
    }
}
