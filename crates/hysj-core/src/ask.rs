//! Ask your mail: find the few passages that may hold the answer, and check
//! the model's answer against them before anyone sees it. The model answers
//! only from these passages; a number, code or quote it gives that isn't
//! written in a cited one means "I couldn't find that" instead.

/// A stored mail to search.
pub struct Doc {
    pub id: String,
    pub from: String,
    pub subject: String,
    pub ts: Option<i64>,
    pub body: String,
}

/// What the model is shown of one mail.
#[derive(Debug, Clone)]
pub struct Passage {
    pub id: String,
    pub from: String,
    pub subject: String,
    pub ts: Option<i64>,
    pub text: String,
}

/// Passages per question: what fits the small models' context.
pub const TOP: usize = 4;
/// Characters of body per passage.
const WINDOW: usize = 900;

const QUESTION_WORDS: &[&str] = &[
    "what", "what's", "whats", "when", "where", "where's", "who", "who's", "how", "which", "why",
    "did", "do", "does", "is", "are", "was", "can", "hva", "hvem", "hvor", "hvordan", "hvilken",
    "hvilke", "hvorfor", "når", "har", "er", "kan",
];

/// A question, not a command: ends in "?" or starts with a question word
/// (English or Norwegian).
pub fn is_question(q: &str) -> bool {
    let q = q.trim();
    if q.ends_with('?') {
        return true;
    }
    let first = q
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_lowercase()
        .replace('’', "'");
    q.split_whitespace().count() >= 3 && QUESTION_WORDS.contains(&first.as_str())
}

const STOP: &[&str] = &[
    "the", "a", "an", "and", "or", "of", "to", "in", "on", "at", "for", "with", "my", "me", "i",
    "we", "our", "you", "your", "it", "is", "are", "was", "did", "do", "does", "last", "that",
    "this", "what", "whats", "when", "where", "who", "how", "which", "why", "there", "from",
    "about", "og", "i", "på", "til", "av", "for", "med", "min", "mitt", "mine", "jeg", "vi", "det",
    "den", "en", "et", "er", "var", "har", "hva", "når", "hvor", "hvem", "hvordan",
];

/// The question's own words worth searching for.
pub fn question_terms(q: &str) -> Vec<String> {
    words(q)
        .filter(|w| w.chars().count() >= 3 && !STOP.contains(&w.as_str()))
        .collect()
}

fn words(s: &str) -> impl Iterator<Item = String> + '_ {
    s.split(|c: char| !c.is_alphanumeric() && c != '§')
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
}

/// The mails that best match `terms`, each cut to the part around its first
/// match. Best first; ties go to the newer mail.
/// ponytail: a scan over every stored body (fine for thousands); SQLite FTS5
/// when stores reach six figures.
pub fn retrieve(docs: &[Doc], terms: &[String]) -> Vec<Passage> {
    let terms: Vec<String> = terms
        .iter()
        .map(|t| t.trim().to_lowercase())
        .filter(|t| t.chars().count() >= 2)
        .collect();
    let mut scored: Vec<(usize, &Doc, Option<usize>)> = docs
        .iter()
        .filter_map(|d| {
            let subject = d.subject.to_lowercase();
            let body = d.body.to_lowercase();
            let mut score = 0;
            let mut first: Option<usize> = None;
            for t in &terms {
                if subject.contains(t.as_str()) {
                    score += 2;
                }
                if let Some(at) = body.find(t.as_str()) {
                    score += 1;
                    first = Some(first.map_or(at, |f| f.min(at)));
                }
            }
            (score > 0).then_some((score, d, first))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.ts.cmp(&a.1.ts)));
    scored
        .into_iter()
        .take(TOP)
        .map(|(_, d, at)| Passage {
            id: d.id.clone(),
            from: d.from.clone(),
            subject: d.subject.clone(),
            ts: d.ts,
            text: window(&d.body, at.unwrap_or(0)),
        })
        .collect()
}

/// About `WINDOW` characters of `body` around byte `at` (found in the
/// lowercased body; close enough, and kept on a character boundary).
fn window(body: &str, at: usize) -> String {
    let chars: Vec<char> = body.chars().collect();
    let at = body
        .char_indices()
        .take_while(|(i, _)| *i < at)
        .count()
        .min(chars.len());
    let start = at.saturating_sub(WINDOW / 3);
    let end = (start + WINDOW).min(chars.len());
    let text: String = chars[start..end].iter().collect();
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The model's answer, before it's checked.
pub struct Guess<'a> {
    pub found: bool,
    pub answer: &'a str,
    /// Indexes into the passages.
    pub sources: &'a [usize],
    /// A short span copied from a passage, if the answer is one.
    pub quote: &'a str,
}

/// An answer that passed the checks.
#[derive(Debug, PartialEq)]
pub struct Checked {
    pub text: String,
    pub sources: Vec<usize>,
    /// Its quote is written word for word in a cited mail.
    pub quoted: bool,
}

/// Keeps the answer only if every number, code and quoted phrase in it is
/// written in the passages it cites.
pub fn check(g: &Guess, passages: &[Passage]) -> Option<Checked> {
    let answer = g.answer.trim();
    if !g.found || answer.is_empty() || g.sources.is_empty() {
        return None;
    }
    let mut sources: Vec<usize> = g.sources.to_vec();
    sources.sort_unstable();
    sources.dedup();
    if sources.iter().any(|&i| i >= passages.len()) {
        return None;
    }
    let cited: String = sources.iter().map(|&i| shown(&passages[i])).collect();
    let hay = squash(&cited).to_lowercase();
    // Every token with a digit in it (amounts, dates, codes, passwords),
    // written as a whole: "279" isn't in "2799".
    let tokens_ok = answer
        .split_whitespace()
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|t| t.chars().any(|c| c.is_ascii_digit()))
        .all(|t| whole(&hay, &t.to_lowercase()));
    let quotes_ok = quoted_spans(answer).all(|q| hay.contains(&squash(q).to_lowercase()));
    if !tokens_ok || !quotes_ok {
        return None;
    }
    let quote = squash(g.quote.trim());
    let quoted = quote.chars().count() >= 3 && squash(&cited).contains(&quote);
    Some(Checked {
        text: answer.to_string(),
        sources,
        quoted,
    })
}

/// A passage as the model sees it, and as its answer is checked against:
/// who, what, when, then the text.
pub fn shown(p: &Passage) -> String {
    let date =
        p.ts.and_then(|t| jiff::Timestamp::from_second(t).ok())
            .map(|t| {
                let z = t.to_zoned(jiff::tz::TimeZone::system());
                format!(
                    "{} ({})",
                    z.strftime("%A %-d %B %Y"),
                    z.strftime("%Y-%m-%d")
                )
            })
            .unwrap_or_default();
    format!(
        "From: {}\nSubject: {}\nDate: {date}\n{}\n",
        p.from, p.subject, p.text
    )
}

/// `needle` in `hay` with no letter or digit right before or after it.
fn whole(hay: &str, needle: &str) -> bool {
    hay.match_indices(needle).any(|(at, _)| {
        let before = hay[..at].chars().next_back();
        let after = hay[at + needle.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Text inside “…”, "…" or «…».
fn quoted_spans(s: &str) -> impl Iterator<Item = &str> {
    let mut out = Vec::new();
    for (open, close) in [('“', '”'), ('"', '"'), ('«', '»')] {
        let mut rest = s;
        while let Some(a) = rest.find(open) {
            let after = &rest[a + open.len_utf8()..];
            let Some(b) = after.find(close) else { break };
            out.push(&after[..b]);
            rest = &after[b + close.len_utf8()..];
        }
    }
    out.into_iter().filter(|q| !q.trim().is_empty())
}

/// The line under an answer: how far to trust it.
pub fn caveat(c: &Checked) -> String {
    match (c.quoted, c.sources.len()) {
        (true, 1) => "Quoted exactly from the email.".into(),
        (true, _) => "Quoted exactly from one of the emails.".into(),
        (false, 1) => "Summarised from 1 email — open it to check.".into(),
        (false, n) => format!("Summarised from {n} emails — open them to check the exact wording."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(id: &str, subject: &str, body: &str, ts: i64) -> Doc {
        Doc {
            id: id.into(),
            from: "Someone".into(),
            subject: subject.into(),
            ts: Some(ts),
            body: body.into(),
        }
    }

    #[test]
    fn questions_and_commands() {
        assert!(is_question("What's the wifi password at the cabin?"));
        assert!(is_question("når betalte jeg rørleggeren"));
        assert!(is_question("wifi?"));
        assert!(!is_question("reply to Aisha"));
        assert!(!is_question("what"));
        assert!(!is_question("latest login code"));
    }

    #[test]
    fn retrieves_and_checks() {
        let docs = [
            doc(
                "1",
                "Hytta: ny ruter",
                "Hei! Nettet heter Hytta-5G og passordet er fjell-og-fjord-24.",
                10,
            ),
            doc("2", "Middag", "Ses på fredag.", 20),
            doc("3", "Ruter-tilbud", "Kjøp ny ruter nå", 30),
        ];
        let p = retrieve(&docs, &["wifi".into(), "passord".into(), "hytta".into()]);
        assert_eq!(p.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["1"]);

        let good = Guess {
            found: true,
            answer: "The network is “Hytta-5G” and the password is fjell-og-fjord-24.",
            sources: &[0],
            quote: "fjell-og-fjord-24",
        };
        assert_eq!(check(&good, &p).map(|c| c.quoted), Some(true));
        // A password the mail doesn't hold is never shown.
        let made_up = Guess {
            answer: "The password is fjell-og-fjord-25.",
            ..good
        };
        assert_eq!(check(&made_up, &p), None);
        let bad_quote = Guess {
            answer: "It's called “Cabin wifi”.",
            ..good
        };
        assert_eq!(check(&bad_quote, &p), None);
        let no_source = Guess {
            sources: &[],
            ..good
        };
        assert_eq!(check(&no_source, &p), None);
        let out_of_range = Guess {
            sources: &[3],
            ..good
        };
        assert_eq!(check(&out_of_range, &p), None);
        let summary = Guess {
            answer: "Your dad sent the new router's details.",
            quote: "",
            ..good
        };
        let c = check(&summary, &p).unwrap();
        assert_eq!(caveat(&c), "Summarised from 1 email — open it to check.");
    }
}
