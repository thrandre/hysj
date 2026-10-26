//! How well the kind classifier sorts: cross-validated on the seed set, then
//! on real mail you labelled. Offline, on a copy of a store (migrated here):
//! `cargo run --release -p hysjd --example kinds_eval -- copy.db labels.tsv`
//! (labels.tsv: message id, tab, kind). Needs `hysj-apple-ai` built
//! (`cargo build -p hysj-app`) or `HYSJ_APPLE_HELPER`.

use hysj_ai::apple::Helper;
use hysj_core::kinds::{self, Classifier, KINDS, MAYBE, SURE};

fn train(v: &[Vec<f32>], y: &[usize]) -> Classifier {
    let ex: Vec<(&[f32], usize, f32)> = v
        .iter()
        .zip(y)
        .map(|(v, k)| (v.as_slice(), *k, 1.0))
        .collect();
    Classifier::train(&ex)
}

/// Rows: truth; columns: predicted (KINDS, then "unsure").
fn report(title: &str, rows: &[(usize, Option<usize>)]) {
    let right = rows.iter().filter(|(t, p)| Some(*t) == *p).count();
    let wrong = rows
        .iter()
        .filter(|(t, p)| p.is_some_and(|p| p != *t))
        .count();
    println!(
        "\n{title}: {right}/{} right, {wrong} wrong, {} unsure",
        rows.len(),
        rows.len() - right - wrong
    );
    println!(
        "{:>13} {}",
        "",
        KINDS
            .iter()
            .map(|k| format!("{:>6.6}", k))
            .collect::<Vec<_>>()
            .join("")
            + "  unsure"
    );
    for (t, name) in KINDS.iter().enumerate() {
        let n = |p: Option<usize>| rows.iter().filter(|r| r.0 == t && r.1 == p).count();
        let cells: String = (0..KINDS.len())
            .map(|p| format!("{:>6}", n(Some(p))))
            .collect();
        println!("{name:>13} {cells}{:>8}", n(None));
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let helper = Helper::spawn()?;
    let seed = kinds::seed();
    let y: Vec<usize> = seed.iter().map(|(k, _)| *k).collect();
    let full_t: Vec<String> = seed.iter().map(|(_, t)| t.clone()).collect();
    let head_t: Vec<String> = seed
        .iter()
        .map(|(_, t)| kinds::head(t).to_string())
        .collect();
    let (model, full) = helper.embed(&full_t).await?;
    let (_, head) = helper.embed(&head_t).await?;
    println!("{} seed examples, {model}", seed.len());

    // Five folds: each example predicted by a classifier that never saw it.
    for (name, v) in [("seed, full text", &full), ("seed, subject+sender", &head)] {
        let mut rows = Vec::new();
        for f in 0..5 {
            let (tr, te): (Vec<usize>, Vec<usize>) = (0..v.len()).partition(|i| i % 5 != f);
            let c = train(
                &tr.iter().map(|i| v[*i].clone()).collect::<Vec<_>>(),
                &tr.iter().map(|i| y[*i]).collect::<Vec<_>>(),
            );
            for i in te {
                let (k, p) = c.predict(&v[i]);
                rows.push((y[i], (p >= SURE).then_some(k)));
            }
        }
        report(&format!("{name} (5-fold)"), &rows);
    }

    let (Some(db), Some(labels)) = (args.get(1), args.get(2)) else {
        return Ok(());
    };
    let store = hysj_core::Store::open(std::path::Path::new(db))?;
    let labels: std::collections::HashMap<String, usize> = std::fs::read_to_string(labels)?
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .filter_map(|(id, k)| Some((id.to_string(), kinds::index(k.trim())?)))
        .collect();
    let all = store.unsorted("eval", 100_000)?;
    let full_c = train(&full, &y);
    let head_c = train(&head, &y);

    // The first pass over every digest mail: how many go on to be read in full.
    let heads: Vec<String> = all
        .iter()
        .map(|m| kinds::text(&m.subject, &m.from, None))
        .collect();
    let mut head_v = Vec::new();
    for c in heads.chunks(64) {
        head_v.extend(helper.embed(c).await?.1);
    }
    let maybe = |v: &[f32]| {
        let p = head_c.probs(v);
        p[0] + p[1] + p[2] >= MAYBE
    };
    let forwarded = head_v.iter().filter(|v| maybe(v)).count();
    println!(
        "\nfirst pass: {forwarded} of {} digest mails read in full",
        all.len()
    );
    for t in [0.02, 0.05, 0.1, 0.15] {
        let n = head_v
            .iter()
            .filter(|v| {
                let p = head_c.probs(v);
                p[0] + p[1] + p[2] >= t
            })
            .count();
        println!("  at {t}: {n}");
    }

    let mut rows = Vec::new();
    let mut missed = 0;
    let mut skipped = Vec::new();
    let mut found = std::collections::HashSet::new();
    for (m, hv) in all.iter().zip(&head_v) {
        let (Some(&truth), Some(body)) = (
            labels.get(&m.id),
            store.raw(&m.id)?.as_deref().and_then(kinds::reading),
        ) else {
            continue;
        };
        if !maybe(hv) {
            if truth <= 2 {
                missed += 1;
                skipped.push((m.addr.to_lowercase(), m.subject.clone()));
                let p = head_c.probs(hv);
                println!(
                    "  first pass missed {} ({:.2}): {}",
                    KINDS[truth],
                    p[0] + p[1] + p[2],
                    m.subject
                );
            }
            rows.push((truth, Some(4)));
            continue;
        }
        let (_, v) = helper
            .embed(&[kinds::text(&m.subject, &m.from, Some(&body))])
            .await?;
        let (k, p) = full_c.predict(&v[0]);
        let pred = (p >= SURE).then_some(k);
        if pred.is_some_and(|k| k <= 2) {
            found.insert(m.addr.to_lowercase());
        }
        if pred.is_some_and(|k| k != truth) {
            println!("  {} as {} ({p:.2}): {}", KINDS[truth], KINDS[k], m.subject);
        }
        rows.push((truth, pred));
    }
    report("your labelled mail (both passes)", &rows);

    // As if you had corrected four fifths of these: train on the seed set
    // plus those (as corrections), sort the fifth you didn't.
    let mut real: Vec<(Vec<f32>, usize)> = Vec::new();
    for m in &all {
        let (Some(&truth), Some(body)) = (
            labels.get(&m.id),
            store.raw(&m.id)?.as_deref().and_then(kinds::reading),
        ) else {
            continue;
        };
        let (_, v) = helper
            .embed(&[kinds::text(&m.subject, &m.from, Some(&body))])
            .await?;
        real.push((v.into_iter().next().unwrap_or_default(), truth));
    }
    let mut rows = Vec::new();
    for f in 0..5 {
        let mut ex: Vec<(&[f32], usize, f32)> = full
            .iter()
            .zip(&y)
            .map(|(v, k)| (v.as_slice(), *k, 1.0))
            .collect();
        ex.extend(
            real.iter()
                .enumerate()
                .filter(|(i, _)| i % 5 != f)
                .map(|(_, (v, k))| (v.as_slice(), *k, 5.0)),
        );
        let c = Classifier::train(&ex);
        for (_, (v, t)) in real.iter().enumerate().filter(|(i, _)| i % 5 == f) {
            let (k, p) = c.predict(v);
            rows.push((*t, (p >= SURE).then_some(k)));
        }
    }
    report("full text, after correcting the rest (5-fold)", &rows);
    for (addr, subject) in &skipped {
        if found.contains(addr) {
            println!("  read in full after all (sender has sent one): {subject}");
        }
    }
    println!("first pass dropped {missed} bookings/orders/subscriptions");
    Ok(())
}
