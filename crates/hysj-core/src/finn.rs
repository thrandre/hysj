//! FINN.no search alerts ("Nye annonser: …") → listings.
//!
//! Each ad appears as two links to `www.finn.no/<ad number>` (through FINN's
//! click tracker): one on the photo, one on the text. The text that follows
//! the second is, in order, the title and the ad's facts. Properties have a
//! price and "Totalpris"; marketplace items a price; jobs neither.
//!
//! ponytail: field order is FINN's current template; a changed template gives
//! fewer facts, never wrong ones (unknown fields are dropped).

use scraper::{ElementRef, Html, Node};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    /// FINN's ad number.
    pub ad: String,
    /// "home", "cabin", "plot", "job", "item".
    pub kind: String,
    pub title: String,
    /// As written ("2 690 000 kr"); empty for jobs.
    pub price: String,
    pub place: String,
    /// The rest, joined ("Eksempelveien 6 · Megler AS", "Employer, Role").
    pub detail: String,
    /// The ad itself, without the click tracker.
    pub link: String,
    /// The ad's photo (remote; only ever shown if the reader allows images).
    pub image: Option<String>,
}

/// The FINN ad number a (tracked) link points at, if any.
fn ad_of(href: &str) -> Option<String> {
    let decoded = percent_decode(href);
    let at = decoded.find("www.finn.no/")? + "www.finn.no/".len();
    let digits: String = decoded[at..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    (digits.len() >= 6).then_some(digits)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

enum Token {
    Ad(String),
    Image(String),
    Text(String),
}

fn walk(e: ElementRef, out: &mut Vec<Token>) {
    let name = e.value().name();
    if matches!(name, "script" | "style" | "head" | "title") {
        return;
    }
    if name == "a"
        && let Some(ad) = e.value().attr("href").and_then(ad_of)
    {
        out.push(Token::Ad(ad));
    }
    if name == "img"
        && let Some(src) = e.value().attr("src")
        && src.contains("finncdn.no")
    {
        out.push(Token::Image(https(src)));
    }
    for child in e.children() {
        match child.value() {
            Node::Text(t) => {
                let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
                if !t.is_empty() {
                    out.push(Token::Text(t));
                }
            }
            Node::Element(_) => {
                if let Some(c) = ElementRef::wrap(child) {
                    walk(c, out);
                }
            }
            _ => {}
        }
    }
}

/// Words that make an ad without a price a property, not a job.
const HOME_WORDS: &[&str] = &[
    "leilighet",
    "bolig",
    "rekkehus",
    "soverom",
    " m²",
    "nybygg",
    "boligprosjekt",
    "selveier",
];

/// A dwelling named in the ad: a house "på stor tomt" is still a house.
const DWELLING_WORDS: &[&str] = &[
    "enebolig",
    "tomannsbolig",
    "generasjonsbolig",
    "leilighet",
    "rekkehus",
    "villa",
    "soverom",
];

/// FINN's photos are on a CDN that serves https too; the app loads no plain http.
pub fn https(url: &str) -> String {
    match url.strip_prefix("http://") {
        Some(rest) => format!("https://{rest}"),
        None => url.to_string(),
    }
}

fn is_price(t: &str) -> bool {
    let t = t.trim();
    t.ends_with(" kr")
        && t[..t.len() - 3]
            .chars()
            .all(|c| c.is_ascii_digit() || c.is_whitespace() || c == '\u{a0}')
}

/// Text that ends an ad's facts (FINN's footer and buttons).
fn ends_ad(t: &str) -> bool {
    let l = t.to_lowercase();
    [
        "se alle",
        "se flere",
        "se annonsene",
        "endre søk",
        "slett søk",
        "lagrede søk",
        "finn.no as",
        "du får denne",
    ]
    .iter()
    .any(|w| l.starts_with(w))
}

/// The listings in a FINN alert's HTML, in order, each once.
pub fn listings(html: &str) -> Vec<Listing> {
    let doc = Html::parse_document(html);
    let mut tokens = Vec::new();
    walk(doc.root_element(), &mut tokens);
    let mut out: Vec<Listing> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let Token::Ad(ad) = &tokens[i] else {
            i += 1;
            continue;
        };
        let ad = ad.clone();
        let mut image = None;
        let mut texts: Vec<String> = Vec::new();
        let mut j = i + 1;
        while j < tokens.len() {
            match &tokens[j] {
                Token::Ad(other) if *other != ad => break,
                Token::Ad(_) => {}
                Token::Image(src) => {
                    image.get_or_insert_with(|| src.clone());
                }
                Token::Text(t) if ends_ad(t) => break,
                Token::Text(t) => texts.push(t.clone()),
            }
            j += 1;
        }
        i = j;
        if texts.is_empty() || out.iter().any(|l| l.ad == ad) {
            continue;
        }
        let title = texts.remove(0);
        let price = texts
            .iter()
            .position(|t| is_price(t))
            .map(|p| texts.remove(p))
            .unwrap_or_default();
        let total = texts
            .iter()
            .position(|t| t.starts_with("Totalpris"))
            .map(|p| texts.remove(p));
        let lower = format!("{title} {}", texts.join(" ")).to_lowercase();
        let cabin = lower.contains("hytte") || lower.contains("fritid");
        let plot = lower.contains("tomt") && !DWELLING_WORDS.iter().any(|w| lower.contains(w));
        let kind = match (&total, price.is_empty()) {
            (Some(_), _) if cabin => "cabin",
            (Some(_), _) if plot => "plot",
            (Some(_), _) => "home",
            // An apartment without FINN's "Totalpris" line is still a home.
            (None, false) if HOME_WORDS.iter().any(|w| lower.contains(w)) => "home",
            (None, false) => "item",
            (None, true) if cabin => "cabin",
            (None, true) if plot => "plot",
            // New-build projects often have no price yet.
            (None, true) if HOME_WORDS.iter().any(|w| lower.contains(w)) => "home",
            (None, true) => "job",
        };
        // Jobs: "Employer, Role" then the place; others: the place first.
        let (place, rest) = if kind == "job" && texts.len() >= 2 {
            (texts[1].clone(), vec![texts[0].clone()])
        } else {
            (
                texts.first().cloned().unwrap_or_default(),
                texts.iter().skip(1).cloned().collect(),
            )
        };
        out.push(Listing {
            link: format!("https://www.finn.no/{ad}"),
            ad,
            kind: kind.into(),
            title,
            price,
            place,
            detail: rest.into_iter().take(2).collect::<Vec<_>>().join(" · "),
            image,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic, shaped like FINN's alert template (two links per ad).
    const ALERT: &str = r#"<html><body><table><tr><td>
<a href="https://click.mailsvc.finn.no/CL0/https:%2F%2Fwww.finn.no%2F452020438%3FfinnMail=agent/1/x"><img src="https://images.finncdn.no/dynamic/480w/item/452020438/a.jpg"></a>
<a href="https://click.mailsvc.finn.no/CL0/https:%2F%2Fwww.finn.no%2F452020438%3FfinnMail=agent/2/x"><h3>Moderne 4-roms med utsikt</h3></a>
<p><span>2 690 000 kr</span></p><p>Gamle Fredrikstad</p><p>Totalpris: 4 650 290 kr</p><p>Eksempelveien 6</p><p>Megler AS</p>
</td></tr><tr><td>
<a href="https://click.mailsvc.finn.no/CL0/https:%2F%2Fwww.finn.no%2F464409516%3Fx/1/y"><img src="http://images.finncdn.no/b.jpg"></a>
<a href="https://click.mailsvc.finn.no/CL0/https:%2F%2Fwww.finn.no%2F464409516%3Fx/2/y"><h3>Prosjektert hytte på Grimsøya</h3></a>
<p>3 100 000 kr</p><p>Skjeberg</p><p>Totalpris: 3 300 000 kr</p>
</td></tr><tr><td>
<a href="https://click.mailsvc.finn.no/CL0/https:%2F%2Fwww.finn.no%2F478833310%3Fs/2/z"><h3>Vil du utvikle systemer som gjør en forskjell?</h3></a>
<p>Eksempel AS, Systemutvikler</p><p>Oslo</p>
</td></tr><tr><td>
<a href="https://click.mailsvc.finn.no/CL0/https:%2F%2Fwww.finn.no%2F400000001%3Fs/2/z"><h3>NYBYGG | 66 leiligheter med utsikt</h3></a>
<p>Eksempelveien 3</p>
</td></tr><tr><td>
<a href="https://click.mailsvc.finn.no/CL0/https:%2F%2Fwww.finn.no%2F400000002%3Fs/2/z"><h3>Innholdsrik enebolig på stor eiet tomt</h3></a>
<p>4 800 000 kr</p><p>Gamle Fredrikstad</p><p>Totalpris: 4 950 000 kr</p>
</td></tr><tr><td>
<a href="https://click.mailsvc.finn.no/CL0/https:%2F%2Fwww.finn.no%2F400000003%3Fs/2/z"><h3>Boligtomt med sjøutsikt</h3></a>
<p>900 000 kr</p><p>Hvaler</p><p>Totalpris: 950 000 kr</p>
</td></tr><tr><td>
<a href="https://click.mailsvc.finn.no/CL0/https:%2F%2Fwww.finn.no%2F400000004%3Fs/2/z"><h3>Begby - Nyere endeleilighet med terrasse</h3></a>
<p>3 500 000 kr</p><p>Gamle Fredrikstad</p>
</td></tr></table>
<a href="https://click.mailsvc.finn.no/CL0/https:%2F%2Fwww.finn.no%2F400000005%3Fs/2/z">Se annonsene på FINN.no</a>
<a href="https://click.mailsvc.finn.no/CL0/https:%2F%2Fwww.finn.no%2Fsearch%2Fsavedsearch">Se alle treff</a></body></html>"#;

    #[test]
    fn reads_an_alert() {
        let l = listings(ALERT);
        assert_eq!(l.len(), 7, "{l:#?}");
        assert_eq!(
            (l[6].kind.as_str(), l[6].detail.as_str()),
            ("home", ""),
            "a flat without Totalpris is a home; FINN's footer isn't a fact"
        );
        assert_eq!(l[3].kind, "home", "a new-build project without a price");
        assert_eq!(l[4].kind, "home", "a house on a large plot");
        assert_eq!(l[5].kind, "plot");
        assert_eq!(
            l[0],
            Listing {
                ad: "452020438".into(),
                kind: "home".into(),
                title: "Moderne 4-roms med utsikt".into(),
                price: "2 690 000 kr".into(),
                place: "Gamle Fredrikstad".into(),
                detail: "Eksempelveien 6 · Megler AS".into(),
                link: "https://www.finn.no/452020438".into(),
                image: Some("https://images.finncdn.no/dynamic/480w/item/452020438/a.jpg".into()),
            }
        );
        assert_eq!(l[1].kind, "cabin");
        assert_eq!(
            l[1].image.as_deref(),
            Some("https://images.finncdn.no/b.jpg"),
            "http photos upgraded"
        );
        assert_eq!(
            (
                l[2].kind.as_str(),
                l[2].place.as_str(),
                l[2].detail.as_str()
            ),
            ("job", "Oslo", "Eksempel AS, Systemutvikler")
        );
        assert!(l[2].price.is_empty());
    }
}
