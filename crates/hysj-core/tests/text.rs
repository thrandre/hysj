use hysj_core::text::strip_quotes;

fn strip(s: &str) -> (String, bool) {
    strip_quotes(&s.replace('|', "\n"))
}

#[test]
fn strips_replies_in_each_language() {
    let cases = [
        (
            "Sounds good!||On Mon, Oct 19, 2026 at 9:00 AM Aisha Rahman <a@x.example> wrote:|> Lunch?",
            "Sounds good!",
        ),
        // Gmail wraps long attributions.
        (
            "Sounds good!||On Mon, Oct 19, 2026 at 9:00 AM Aisha Rahman <|a@x.example> wrote:|> Lunch?",
            "Sounds good!",
        ),
        (
            "Høres bra ut!||man. 19. okt. 2026 kl. 09:00 skrev Aisha Rahman <a@x.example>:|> Lunsj?",
            "Høres bra ut!",
        ),
        (
            "Låter bra!||Den 19 okt. 2026 kl. 09:00 skrev Aisha <a@x.example>:|> Lunch?",
            "Låter bra!",
        ),
        (
            "Ja.||Fra: Aisha Rahman <a@x.example>|Sendt: mandag 19. oktober 2026 09:00|Til: Me|Emne: Lunsj",
            "Ja.",
        ),
        ("Yes.|-----Original Message-----|From: Aisha", "Yes."),
        (
            "See you there.|-- |Aisha Rahman|CEO, Example",
            "See you there.",
        ),
        ("On my way|Sendt fra min iPhone", "On my way"),
        (
            "> Lunch?|Yes, 12:00.|> Where?|The usual.",
            "Yes, 12:00.\nThe usual.",
        ),
    ];
    for (input, want) in cases {
        assert_eq!(strip(input), (want.to_string(), true), "input: {input}");
    }
}

#[test]
fn leaves_ordinary_text_alone() {
    assert_eq!(
        strip("Hi!|Here's what he wrote: nothing.|Bye"),
        ("Hi!\nHere's what he wrote: nothing.\nBye".into(), false)
    );
    assert_eq!(
        strip("Fra: is Norwegian for from.|That's all."),
        ("Fra: is Norwegian for from.\nThat's all.".into(), false)
    );
}

#[test]
fn falls_back_to_raw_when_nothing_is_left() {
    let raw = "> only a quote|> nothing new";
    assert_eq!(strip(raw), (raw.replace('|', "\n"), false));
}

#[test]
fn forwards_are_kept() {
    let body = "Se under.\n\n---------- Forwarded message ---------\nFra: Siri <siri@example.no>\nDate: man. 19. okt. 2026\nSubject: Innkalling\n\nVelkommen til styremøte 28. oktober kl. 19:30.\n\nMvh\nSiri";
    let (text, _) = hysj_core::text::strip_quotes(body);
    assert!(text.contains("styremøte 28. oktober"), "{text}");
}
