//! A message as the app shows it: the original's HTML (or text), sanitised.
//!
//! The app renders this in a sandboxed frame without scripts, under a CSP that
//! blocks remote loads until the reader allows images. Here, with ammonia: no
//! scripts, event handlers, forms, frames or base/meta/link; links only
//! http(s)/mailto; relative URLs dropped; embedded (`cid:`) images inlined as
//! data URIs; remote images counted. `<style>` blocks stay (newsletters need
//! them); CSS can't run code, and the frame's CSP stops it loading anything.
//! Quotes and signatures are folded by the app, which can see the layout.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use base64::Engine;
use hysj_proto::AttachmentInfo;
use mail_parser::{MessageParser, MimeHeaders, PartType};

/// Embedded images above this are left out (a broken image beats megabytes
/// over the socket).
const MAX_EMBEDDED: usize = 2 * 1024 * 1024;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct View {
    pub html: String,
    pub plain: bool,
    pub attachments: Vec<AttachmentInfo>,
    pub remote_images: u32,
}

/// Renders a raw RFC 822 message for the app.
pub fn view(raw: &[u8]) -> View {
    let Some(msg) = MessageParser::default().parse(raw) else {
        return View::default();
    };
    // Images the HTML can point at with `cid:`, as data URIs.
    let mut cids: HashMap<String, (String, usize)> = HashMap::new();
    for (i, part) in msg.attachments().enumerate() {
        if let (Some(id), Some(ct)) = (part.content_id(), part.content_type())
            && ct.ctype().eq_ignore_ascii_case("image")
            && part.contents().len() <= MAX_EMBEDDED
        {
            let uri = format!(
                "data:image/{};base64,{}",
                ct.subtype().unwrap_or("png").to_ascii_lowercase(),
                base64::engine::general_purpose::STANDARD.encode(part.contents())
            );
            cids.insert(id.trim_matches(['<', '>']).to_string(), (uri, i));
        }
    }
    let html_part = msg.html_bodies().find_map(|p| match &p.body {
        PartType::Html(h) => Some(h.to_string()),
        _ => None,
    });
    let (html, plain, remote, used) = match html_part {
        Some(h) => {
            let (html, remote, used) = sanitize(&h, &cids);
            (html, false, remote, used)
        }
        None => {
            let text = msg.body_text(0).map(|t| t.into_owned()).unwrap_or_default();
            (text_html(&text), true, 0, HashSet::new())
        }
    };
    let attachments = msg
        .attachments()
        .enumerate()
        .filter(|(i, _)| !used.contains(i))
        .map(|(i, part)| AttachmentInfo {
            index: i as u32,
            name: part_name(part),
            mime: part_mime(part),
            size: part.contents().len() as u32,
        })
        .collect();
    View {
        html,
        plain,
        attachments,
        remote_images: remote,
    }
}

fn part_mime(part: &mail_parser::MessagePart) -> String {
    part.content_type()
        .map(|c| format!("{}/{}", c.ctype(), c.subtype().unwrap_or("octet-stream")))
        .unwrap_or_else(|| "application/octet-stream".into())
        .to_ascii_lowercase()
}

fn part_name(part: &mail_parser::MessagePart) -> String {
    part.attachment_name()
        .map(str::to_string)
        .or_else(|| match &part.body {
            PartType::Message(m) => m.subject().map(|s| format!("{s}.eml")),
            _ => None,
        })
        .unwrap_or_else(|| "attachment".into())
}

/// The bytes of attachment `index` (as listed by [`view`]): (name, mime, data).
pub fn attachment(raw: &[u8], index: u32) -> Option<(String, String, Vec<u8>)> {
    let msg = MessageParser::default().parse(raw)?;
    let part = msg.attachment(index)?;
    let data = match &part.body {
        // A forwarded message, whole, as .eml.
        PartType::Message(m) => m.raw_message().to_vec(),
        _ => part.contents().to_vec(),
    };
    Some((part_name(part), part_mime(part), data))
}

/// Sanitised HTML, the number of remote images, and which attachments it
/// shows inline.
fn sanitize(html: &str, cids: &HashMap<String, (String, usize)>) -> (String, u32, HashSet<usize>) {
    let remote = Arc::new(AtomicU32::new(0));
    let used = Arc::new(std::sync::Mutex::new(HashSet::new()));
    let cid_uris: Arc<HashMap<String, (String, usize)>> = Arc::new(cids.clone());
    let mut b = ammonia::Builder::default();
    b.rm_clean_content_tags(&["style"])
        .add_clean_content_tags(&["title"])
        .add_tags(&[
            "style",
            "center",
            "font",
            "span",
            "div",
            "table",
            "thead",
            "tbody",
            "tfoot",
            "tr",
            "td",
            "th",
            "caption",
            "col",
            "colgroup",
            "img",
            "hr",
            "br",
            "u",
            "s",
            "small",
            "big",
            "sup",
            "sub",
            "section",
            "article",
            "header",
            "footer",
            "main",
            "figure",
            "figcaption",
        ])
        .add_generic_attributes(&[
            "style",
            "class",
            "id",
            "align",
            "valign",
            "width",
            "height",
            "bgcolor",
            "color",
            "border",
            "cellpadding",
            "cellspacing",
            "dir",
            "lang",
            "title",
            "face",
            "size",
            "role",
        ])
        .add_tag_attributes("img", &["src", "alt", "width", "height"])
        .add_tag_attributes("a", &["href", "title"])
        // `cid:` reaches the filter below, which inlines or drops it.
        .url_schemes(HashSet::from(["http", "https", "mailto", "data", "cid"]))
        .url_relative(ammonia::UrlRelative::Deny)
        .link_rel(Some("noopener noreferrer"));
    {
        let remote = remote.clone();
        let used = used.clone();
        let cid_uris = cid_uris.clone();
        b.attribute_filter(move |element, attribute, value| {
            if element != "img" || attribute != "src" {
                return Some(value.into());
            }
            let v = value.trim();
            if let Some(cid) = v.strip_prefix("cid:") {
                let (uri, i) = cid_uris.get(cid)?;
                used.lock().expect("not poisoned").insert(*i);
                return Some(uri.clone().into());
            }
            let lower = v.to_ascii_lowercase();
            if lower.starts_with("https://") {
                remote.fetch_add(1, Ordering::Relaxed);
                return Some(value.into());
            }
            // The frame loads no plain http; most image hosts serve https too.
            if lower.starts_with("http://") {
                remote.fetch_add(1, Ordering::Relaxed);
                return Some(format!("https://{}", &v[7..]).into());
            }
            // Inline images only; anything else is dropped.
            lower.starts_with("data:image/").then(|| value.into())
        });
    }
    let out = b.clean(html).to_string();
    let used = used.lock().expect("not poisoned").clone();
    (out, remote.load(Ordering::Relaxed), used)
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// Escaped text with bare http(s) URLs linked.
fn linkify(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = ["https://", "http://"]
        .iter()
        .filter_map(|p| rest.find(p))
        .min()
    {
        let (before, url_on) = rest.split_at(at);
        let end = url_on
            .find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | ')' | ']'))
            .unwrap_or(url_on.len());
        let mut url = &url_on[..end];
        // A sentence's full stop or comma isn't part of the link.
        while url.ends_with(['.', ',', ';', ':', '!', '?']) {
            url = &url[..url.len() - 1];
        }
        out.push_str(&escape(before));
        let u = escape(url);
        out.push_str(&format!("<a href=\"{u}\">{u}</a>"));
        rest = &url_on[url.len()..];
    }
    out.push_str(&escape(rest));
    out
}

/// Plain text as HTML: the sender's part, then quotes and signature in a
/// block the app folds.
pub fn text_html(body: &str) -> String {
    let (kept, rest) = crate::text::split_quotes(body);
    let mut html = format!(
        "<div class=\"hysj-plain\">{}</div>",
        linkify(&kept.join("\n"))
    );
    if !rest.is_empty() {
        html.push_str(&format!(
            "<div class=\"hysj-plain hysj-quoted\">{}</div>",
            linkify(&rest.join("\n"))
        ));
    }
    html
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mail(content_type: &str, body: &str) -> Vec<u8> {
        format!("From: a@example.com\r\nTo: b@example.com\r\nSubject: s\r\nMIME-Version: 1.0\r\nContent-Type: {content_type}\r\n\r\n{body}").into_bytes()
    }

    #[test]
    fn html_is_defused() {
        let raw = mail(
            "text/html; charset=utf-8",
            r#"<html><head><title>Hidden title</title><style>p{color:red}</style><script>alert(1)</script></head>
<body onload="x()"><p onclick="steal()">Hei <a href="javascript:alert(1)">bad</a> <a href="https://example.com/x" onmouseover="y()">good</a></p>
<form action="https://evil.example"><input name=pw></form><iframe src="https://evil.example"></iframe>
<img src="https://tracker.example/p.gif" width="1" height="1"><img src="http://cdn.example/hero.png" alt="Hero">
<a href="/relative">rel</a><base href="https://evil.example/"><meta http-equiv="refresh" content="0;url=https://evil.example"></body></html>"#,
        );
        let v = view(&raw);
        let h = v.html.to_lowercase();
        for bad in [
            "<script",
            "alert(1)",
            "onload",
            "onclick",
            "onmouseover",
            "javascript:",
            "<form",
            "<input",
            "<iframe",
            "<base",
            "<meta",
            "hidden title",
            "/relative",
        ] {
            assert!(!h.contains(bad), "{bad} survived: {}", v.html);
        }
        assert!(
            h.contains("<style>p{color:red}</style>"),
            "styles kept: {}",
            v.html
        );
        assert!(h.contains(r#"href="https://example.com/x""#));
        assert_eq!(v.remote_images, 2);
        assert!(
            h.contains(r#"src="https://cdn.example/hero.png""#),
            "http upgraded"
        );
        assert!(!v.plain);
    }

    #[test]
    fn embedded_images_inline_and_attachments_listed() {
        let raw = b"From: a@example.com\r\nTo: b@example.com\r\nSubject: s\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=X\r\n\r\n--X\r\nContent-Type: multipart/related; boundary=Y\r\n\r\n--Y\r\nContent-Type: text/html\r\n\r\n<p>Logo <img src=\"cid:logo@x\"></p>\r\n--Y\r\nContent-Type: image/png\r\nContent-ID: <logo@x>\r\nContent-Transfer-Encoding: base64\r\n\r\niVBORw0KGgo=\r\n--Y--\r\n--X\r\nContent-Type: application/pdf; name=\"invoice.pdf\"\r\nContent-Disposition: attachment; filename=\"invoice.pdf\"\r\nContent-Transfer-Encoding: base64\r\n\r\nJVBERi0=\r\n--X--\r\n";
        let v = view(raw);
        assert!(
            v.html.contains("src=\"data:image/png;base64,"),
            "{}",
            v.html
        );
        assert_eq!(v.attachments.len(), 1, "{:?}", v.attachments);
        assert_eq!(v.attachments[0].name, "invoice.pdf");
        let (name, mime, data) = attachment(raw, v.attachments[0].index).unwrap();
        assert_eq!(
            (name.as_str(), mime.as_str()),
            ("invoice.pdf", "application/pdf")
        );
        assert_eq!(data, b"%PDF-");
    }

    #[test]
    fn plain_text_escaped_linked_and_folded() {
        let raw = mail(
            "text/plain; charset=utf-8",
            "Se <her>: https://example.com/a.\r\n\r\nOn Mon, Aisha wrote:\r\n> earlier\r\n",
        );
        let v = view(&raw);
        assert!(v.plain);
        assert!(v.html.contains("&lt;her&gt;"));
        assert!(
            v.html
                .contains(r#"<a href="https://example.com/a">https://example.com/a</a>."#),
            "{}",
            v.html
        );
        assert!(v.html.contains("hysj-quoted"), "{}", v.html);
    }
}
