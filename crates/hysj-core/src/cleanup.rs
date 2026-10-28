//! Clean up: which subscriptions you read (opened in Hysj), how each one can
//! be unsubscribed from, least read first.

use std::collections::HashMap;

use hysj_proto::Subscription;

/// How a sender lets you unsubscribe (from `List-Unsubscribe` and
/// `List-Unsubscribe-Post`).
#[derive(Debug, Clone, PartialEq)]
pub enum Method {
    /// RFC 8058: a POST to this https URL, no page, no form.
    OneClick(String),
    /// An email to this address (with `?subject=` if given).
    Mailto(String),
    /// Their page, opened in the browser.
    Web(String),
    None,
}

impl Method {
    pub fn parse(list_unsubscribe: &str, post: Option<&str>) -> Self {
        let targets: Vec<&str> = list_unsubscribe
            .split(',')
            .filter_map(|t| {
                let t = t.trim();
                t.strip_prefix('<')?.strip_suffix('>')
            })
            .map(str::trim)
            .collect();
        let https = targets.iter().find(|t| t.starts_with("https://"));
        let one_click = post.is_some_and(|p| p.to_lowercase().contains("one-click"));
        if let (Some(u), true) = (https, one_click) {
            return Self::OneClick(u.to_string());
        }
        if let Some(m) = targets.iter().find_map(|t| {
            t.get(..7)
                .filter(|p| p.eq_ignore_ascii_case("mailto:"))
                .map(|_| &t[7..])
        }) {
            return Self::Mailto(m.to_string());
        }
        match targets
            .iter()
            .find(|t| t.starts_with("http://") || t.starts_with("https://"))
        {
            Some(u) => Self::Web(u.to_string()),
            None => Self::None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::OneClick(_) => "one_click",
            Self::Mailto(_) => "mailto",
            Self::Web(_) => "web",
            Self::None => "none",
        }
    }
}

/// One bulk mail of the last 90 days.
pub struct Bulk {
    pub address: String,
    pub name: String,
    pub date: i64,
    pub opened: bool,
    /// From its headers (the newest mail's are used).
    pub method: Method,
}

/// Mail since counting began that makes "never opened" fair to say.
pub const ENOUGH: u32 = 4;
pub const DAYS: i64 = 90;

/// Subscriptions from the last 90 days' bulk mail, least read first.
/// `states`: the reader's choices by address.
pub fn subscriptions(
    mail: &[Bulk],
    states: &HashMap<String, String>,
    counting_since: i64,
) -> Vec<Subscription> {
    let mut by: HashMap<&str, Vec<&Bulk>> = HashMap::new();
    for m in mail {
        by.entry(m.address.as_str()).or_default().push(m);
    }
    let mut out: Vec<Subscription> = by
        .into_iter()
        .map(|(address, ms)| {
            let newest = ms.iter().max_by_key(|m| m.date).expect("one at least");
            let counted: Vec<_> = ms.iter().filter(|m| m.date >= counting_since).collect();
            let opened = counted.iter().filter(|m| m.opened).count() as u32;
            let counted = counted.len() as u32;
            let per_week = ms.len() as f32 * 7.0 / DAYS as f32;
            Subscription {
                address: address.to_string(),
                name: newest.name.clone(),
                per_week,
                frequency: frequency(per_week),
                opened,
                counted,
                never: counted >= ENOUGH && opened == 0,
                method: newest.method.name().into(),
                state: states
                    .get(address)
                    .cloned()
                    .unwrap_or_else(|| "active".into()),
            }
        })
        .collect();
    let rate = |s: &Subscription| {
        if s.counted == 0 {
            1.0
        } else {
            s.opened as f32 / s.counted as f32
        }
    };
    out.sort_by(|a, b| {
        rate(a)
            .total_cmp(&rate(b))
            .then(b.per_week.total_cmp(&a.per_week))
            .then(a.name.cmp(&b.name))
    });
    out
}

/// "3 a week", "daily", "weekly", "monthly".
pub fn frequency(per_week: f32) -> String {
    match per_week {
        w if w >= 6.0 => "daily".into(),
        w if w >= 1.5 => format!("{} a week", w.round()),
        w if w >= 0.75 => "weekly".into(),
        w if w >= 0.4 => "every other week".into(),
        _ => "monthly".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn methods() {
        let url = "https://news.example.com/u/1";
        assert_eq!(
            Method::parse(
                &format!("<mailto:u@news.example.com>, <{url}>"),
                Some("List-Unsubscribe=One-Click")
            ),
            Method::OneClick(url.into())
        );
        assert_eq!(
            Method::parse(
                &format!("<mailto:u@news.example.com?subject=stop>, <{url}>"),
                None
            ),
            Method::Mailto("u@news.example.com?subject=stop".into())
        );
        assert_eq!(
            Method::parse(&format!("<{url}>"), None),
            Method::Web(url.into())
        );
        assert_eq!(Method::parse("junk", None), Method::None);
    }

    #[test]
    fn least_read_first() {
        let day = 86_400;
        let mail = |a: &'static str, n: i64, opened: i64| {
            (0..n).map(move |i| Bulk {
                address: a.into(),
                name: a.into(),
                date: 100 * day + i * day,
                opened: i < opened,
                method: Method::None,
            })
        };
        let all: Vec<Bulk> = mail("read", 5, 5)
            .chain(mail("never", 6, 0))
            .chain(mail("new", 2, 0))
            .collect();
        let s = subscriptions(&all, &HashMap::new(), 100 * day);
        assert_eq!(
            s.iter()
                .map(|s| (s.address.as_str(), s.never))
                .collect::<Vec<_>>(),
            [("never", true), ("new", false), ("read", false)]
        );
        assert_eq!(frequency(s[0].per_week), "every other week");
        assert_eq!(frequency(3.2), "3 a week");
    }
}
