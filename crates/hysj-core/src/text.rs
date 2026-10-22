//! Message text for display: quoted replies and signatures stripped by rules.

/// The new part of a reply, and whether anything was hidden. Falls back to the
/// full text when stripping would leave nothing (principle: show the raw thing).
pub fn strip_quotes(body: &str) -> (String, bool) {
    let (kept, rest) = split_quotes(body);
    let text = kept.join("\n");
    if text.trim().is_empty() {
        return (body.trim_end().to_string(), false);
    }
    (text, !rest.is_empty())
}

/// A message's lines split into what the sender wrote (forwarded content
/// included) and what quotes earlier mail or signs it off.
pub fn split_quotes(body: &str) -> (Vec<&str>, Vec<&str>) {
    let lines: Vec<&str> = body.lines().collect();
    let mut kept: Vec<&str> = Vec::new();
    let mut rest: Vec<&str> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let next = lines.get(i + 1).copied().unwrap_or("");
        // A forward: everything from its header on is what was sent to you.
        if is_forward_header(line) {
            kept.extend(&lines[i..]);
            break;
        }
        if ends_message(line, next, &lines[i..]) {
            rest.extend(&lines[i..]);
            break;
        }
        if line.trim_start().starts_with('>') {
            rest.push(line);
            continue;
        }
        kept.push(line);
    }
    while kept.last().is_some_and(|l| l.trim().is_empty()) {
        kept.pop();
    }
    if kept.iter().all(|l| l.trim().is_empty()) {
        return (lines, Vec::new());
    }
    (kept, rest)
}

/// "---------- Forwarded message ---------", "Begin forwarded message:", and translations.
pub fn is_forward_header(line: &str) -> bool {
    let t = line.trim().trim_start_matches(['-', ' ']).to_lowercase();
    [
        "forwarded message",
        "videresendt melding",
        "vidarebefordrat meddelande",
        "videresendt meddelelse",
        "begin forwarded message",
    ]
    .iter()
    .any(|p| t.starts_with(p))
}

/// Does the reply's own text end at this line?
fn ends_message(line: &str, next: &str, rest: &[&str]) -> bool {
    let t = line.trim();
    let lower = t.to_lowercase();
    // Signature delimiter and mobile footers.
    if t == "--" || line == "-- " {
        return true;
    }
    if ["sent from my ", "sendt fra min ", "skickat från min "]
        .iter()
        .any(|p| lower.starts_with(p))
    {
        return true;
    }
    // "-----Original Message-----" and translations — but not a forward's
    // "---------- Forwarded message ---------": that's what was sent to you.
    let forward = ["forward", "videresend", "vidarebefordra"]
        .iter()
        .any(|w| lower.contains(w));
    if !forward && lower.starts_with("-----") && lower.ends_with("-----") && lower.len() > 10 {
        return true;
    }
    // Attribution: "On … wrote:" / "… skrev …:" (Norwegian, Swedish, Danish),
    // possibly wrapped onto the next line.
    let attribution = |s: &str| {
        let s = s.trim().to_lowercase();
        s.ends_with(':') && (s.contains(" wrote") || s.contains("skrev ") || s.contains(" skrev"))
    };
    let wrapped = format!("{t} {}", next.trim());
    if !t.is_empty() && (attribution(t) || attribution(&wrapped)) && looks_like_attribution_start(t)
    {
        return true;
    }
    // Outlook header block: "From:/Fra:/Från:" followed by "Sent:/Sendt:/Skickat:" or "To:/Til:/Till:".
    if ["from:", "fra:", "från:"]
        .iter()
        .any(|p| lower.starts_with(p))
    {
        return rest.iter().skip(1).take(4).any(|l| {
            let l = l.trim().to_lowercase();
            [
                "sent:", "sendt:", "skickat:", "to:", "til:", "till:", "date:", "dato:", "datum:",
            ]
            .iter()
            .any(|p| l.starts_with(p))
        });
    }
    false
}

/// First line of a wrapped attribution: "On …", "Den …", or a Gmail Norwegian
/// date ("man. 19. okt. 2026 kl. 09:00 skrev …").
fn looks_like_attribution_start(t: &str) -> bool {
    let l = t.to_lowercase();
    l.starts_with("on ") || l.starts_with("den ") || l.contains(" kl. ") || l.starts_with("am ")
}
