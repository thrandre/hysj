//! What kind of mail this is — a booking, an order, a subscription, an offer
//! or something else — decided by a small classifier over sentence vectors
//! (Apple's NaturalLanguage embeddings, from the daemon). It's trained on an
//! invented seed set (`data/kinds.jsonl`) plus the reader's corrections, and
//! retrained in a blink whenever those change. The language model only finds
//! the facts in a mail this has already sorted; it never decides the kind.

use serde::Deserialize;

pub const KINDS: [&str; 5] = ["booking", "order", "subscription", "offer", "other"];

/// Below this, the classifier isn't sure: nothing is filed.
pub const SURE: f32 = 0.6;

/// The first pass (subject and sender only) lets a mail through to the
/// full-text pass when it gives a booking/order/subscription at least this.
pub const MAYBE: f32 = 0.1;

const SEED: &str = include_str!("../data/kinds.jsonl");

#[derive(Deserialize)]
struct SeedLine {
    kind: String,
    text: String,
}

/// The invented examples: (kind index, full text).
pub fn seed() -> Vec<(usize, String)> {
    SEED.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<SeedLine>(l).expect("data/kinds.jsonl is valid"))
        .map(|s| (index(&s.kind).expect("a known kind"), s.text))
        .collect()
}

pub fn index(kind: &str) -> Option<usize> {
    KINDS.iter().position(|k| *k == kind)
}

/// A mail as the classifier reads it (the seed set's format). Without a
/// body it's the first pass's text.
pub fn text(subject: &str, from: &str, body: Option<&str>) -> String {
    match body {
        Some(b) => format!("Subject: {subject}\nFrom: {from}\n\n{}", words(b)),
        None => format!("Subject: {subject}\nFrom: {from}"),
    }
}

/// The text a mail is read by: its HTML as text when it has HTML (a
/// plain-text part is often just "view this in a browser"), else its text.
pub fn reading(raw: &[u8]) -> Option<String> {
    let m = mail_parser::MessageParser::default().parse(raw)?;
    match m.body_html(0) {
        Some(h) => Some(mail_parser::decoders::html::html_to_text(&h)),
        None => m.body_text(0).map(|t| t.into_owned()),
    }
}

/// A body's words, without what fills the model's few hundred tokens and
/// says nothing: links, rules (`=====`) and runs of blank space.
fn words(body: &str) -> String {
    body.lines()
        .map(|l| {
            l.split_whitespace()
                .filter(|w| {
                    !w.contains("://") && !w.starts_with("www.") && !w.starts_with("mailto:")
                })
                .filter(|w| w.chars().any(char::is_alphanumeric))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The first pass's text of a seed example: its subject and sender lines.
pub fn head(text: &str) -> &str {
    text.split("\n\n").next().unwrap_or(text)
}

/// Softmax regression on standardised vectors.
/// ponytail: full-batch gradient descent; a few hundred examples train in
/// well under a second. Switch to minibatches if corrections reach thousands.
#[derive(Debug, Clone)]
pub struct Classifier {
    mean: Vec<f32>,
    scale: Vec<f32>,
    w: Vec<Vec<f32>>,
    b: Vec<f32>,
}

impl Classifier {
    /// `examples`: (vector, kind index, weight). Corrections weigh more than seeds.
    pub fn train(examples: &[(&[f32], usize, f32)]) -> Self {
        let n = examples.len().max(1) as f32;
        let d = examples.first().map_or(0, |e| e.0.len());
        let k = KINDS.len();
        let mut mean = vec![0f32; d];
        for (v, _, _) in examples {
            for (m, x) in mean.iter_mut().zip(v.iter()) {
                *m += x / n;
            }
        }
        let mut scale = vec![0f32; d];
        for (v, _, _) in examples {
            for ((s, x), m) in scale.iter_mut().zip(v.iter()).zip(&mean) {
                *s += (x - m).powi(2) / n;
            }
        }
        for s in &mut scale {
            *s = 1.0 / (s.sqrt() + 1e-6);
        }
        let xs: Vec<Vec<f32>> = examples
            .iter()
            .map(|(v, _, _)| standardise(v, &mean, &scale))
            .collect();
        let total: f32 = examples.iter().map(|e| e.2).sum::<f32>().max(1e-6);
        let mut c = Self {
            mean,
            scale,
            w: vec![vec![0f32; d]; k],
            b: vec![0f32; k],
        };
        let (rate, l2) = (0.5f32, 1e-3f32);
        for _ in 0..300 {
            let mut gw = vec![vec![0f32; d]; k];
            let mut gb = vec![0f32; k];
            for (x, (_, y, weight)) in xs.iter().zip(examples) {
                let p = c.probs_std(x);
                for j in 0..k {
                    let g = (p[j] - if j == *y { 1.0 } else { 0.0 }) * weight / total;
                    gb[j] += g;
                    for (gwi, xi) in gw[j].iter_mut().zip(x) {
                        *gwi += g * xi;
                    }
                }
            }
            for j in 0..k {
                c.b[j] -= rate * gb[j];
                for (wi, gi) in c.w[j].iter_mut().zip(&gw[j]) {
                    *wi -= rate * (gi + l2 * *wi);
                }
            }
        }
        c
    }

    fn probs_std(&self, x: &[f32]) -> Vec<f32> {
        let z: Vec<f32> = self
            .w
            .iter()
            .zip(&self.b)
            .map(|(w, b)| w.iter().zip(x).map(|(a, b)| a * b).sum::<f32>() + b)
            .collect();
        let max = z.iter().copied().fold(f32::MIN, f32::max);
        let e: Vec<f32> = z.iter().map(|v| (v - max).exp()).collect();
        let s: f32 = e.iter().sum();
        e.into_iter().map(|v| v / s).collect()
    }

    /// Probability per kind (in `KINDS` order).
    pub fn probs(&self, v: &[f32]) -> Vec<f32> {
        self.probs_std(&standardise(v, &self.mean, &self.scale))
    }

    /// The likeliest kind and how sure it is.
    pub fn predict(&self, v: &[f32]) -> (usize, f32) {
        let p = self.probs(v);
        p.iter()
            .copied()
            .enumerate()
            .fold((0, 0.0), |a, (i, x)| if x > a.1 { (i, x) } else { a })
    }
}

fn standardise(v: &[f32], mean: &[f32], scale: &[f32]) -> Vec<f32> {
    v.iter()
        .zip(mean)
        .zip(scale)
        .map(|((x, m), s)| (x - m) * s)
        .collect()
}

/// Vectors as stored (little-endian f32s).
pub fn to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

pub fn from_blob(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learns_separable_kinds() {
        // Five clusters in 8 dimensions, a little noise.
        let mut data = Vec::new();
        for k in 0..KINDS.len() {
            for i in 0..12 {
                let mut v = vec![0.1f32; 8];
                v[k] = 1.0;
                v[7] = (i as f32) * 0.01;
                data.push((v, k));
            }
        }
        let ex: Vec<(&[f32], usize, f32)> =
            data.iter().map(|(v, k)| (v.as_slice(), *k, 1.0)).collect();
        let c = Classifier::train(&ex);
        let mut probe = vec![0.1f32; 8];
        probe[2] = 0.9;
        let (k, p) = c.predict(&probe);
        assert_eq!((KINDS[k], p > SURE), ("subscription", true));
        assert_eq!(from_blob(&to_blob(&probe)), probe);
        let raw = b"Subject: x\r\nContent-Type: multipart/alternative; boundary=b\r\n\r\n--b\r\nContent-Type: text/plain\r\n\r\nView online\r\n--b\r\nContent-Type: text/html\r\n\r\n<p>Kvittering: sokker</p>\r\n--b--\r\n";
        assert!(reading(raw).unwrap().contains("Kvittering: sokker"));
        assert_eq!(
            text(
                "Kvittering",
                "Shop",
                Some("Takk!\r\n\r\n=====\nSe https://x.example/a?b  her\n")
            ),
            "Subject: Kvittering\nFrom: Shop\n\nTakk!\nSe her"
        );
    }

    #[test]
    fn seed_set_is_balanced() {
        let s = seed();
        for (k, name) in KINDS.iter().enumerate() {
            assert!(s.iter().filter(|(i, _)| *i == k).count() >= 40, "{name}");
        }
        assert!(s.iter().all(|(_, t)| t.starts_with("Subject: ")));
    }
}
