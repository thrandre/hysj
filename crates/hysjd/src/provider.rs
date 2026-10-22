//! Mail providers as data: who an address belongs to, its servers, and how
//! to sign in (password, app password or OAuth). Everything else is generic
//! IMAP/SMTP; server quirks (Gmail labels, CONDSTORE) are read from the
//! server's capabilities, not from here. Unknown domains get guessed servers.
//! ponytail: a built-in list; Thunderbird's ISPDB lookup for the rest is next.

use hysj_proto::ProviderInfo;

use crate::oauth::OAuthConfig;

/// Hysj's app registration with Microsoft (Entra ID, personal Microsoft
/// accounts, public client, `http://localhost` redirect). Not a secret.
/// `HYSJ_MS_CLIENT_ID` at build time overrides it (a fork's own registration).
pub const MICROSOFT_OAUTH: OAuthConfig = OAuthConfig {
    key: "ms",
    name: "Microsoft",
    authorize: "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize",
    token: "https://login.microsoftonline.com/consumers/oauth2/v2.0/token",
    scopes: "https://outlook.office.com/IMAP.AccessAsUser.All https://outlook.office.com/SMTP.Send offline_access",
    client_id: match option_env!("HYSJ_MS_CLIENT_ID") {
        Some(id) => id,
        None => "e0bb8300-4325-43b9-803e-c6a894c4f02f",
    },
    client_secret: "",
    extra: &[("prompt", "select_account")],
    notes: &[
        "Outlook.com and Hotmail don’t take passwords from mail apps any more.",
        "Microsoft may say Hysj isn’t verified; that’s expected for a small app. Accept, then allow Hysj to read and send your mail.",
    ],
};

/// Hysj's Google Cloud OAuth client (desktop app, Gmail's IMAP/SMTP scope;
/// unverified, so Google warns and allows 100 people). The client secret
/// comes from `HYSJ_GOOGLE_CLIENT_SECRET` at build time (a release secret on
/// GitHub; the keychain locally): without it, Gmail is command line only.
pub const GOOGLE_OAUTH: OAuthConfig = OAuthConfig {
    key: "google",
    name: "Google",
    authorize: "https://accounts.google.com/o/oauth2/v2/auth",
    token: "https://oauth2.googleapis.com/token",
    scopes: "https://mail.google.com/",
    client_id: match option_env!("HYSJ_GOOGLE_CLIENT_ID") {
        Some(id) => id,
        None => "804814055127-26et2p3r52q1nt7l8tcbo4t7l2tsaafm.apps.googleusercontent.com",
    },
    client_secret: match option_env!("HYSJ_GOOGLE_CLIENT_SECRET") {
        Some(s) => s,
        None => "",
    },
    // A refresh token, and asked for again when one is missing.
    extra: &[("access_type", "offline"), ("prompt", "consent")],
    notes: &[
        "Google will say it hasn’t verified Hysj — it’s a small app Google hasn’t reviewed. Choose Advanced, then Go to Hysj.",
        "Then allow Hysj to read, send and manage your mail.",
    ],
};

pub enum Auth {
    Password,
    /// A provider-made password just for Hysj; where to make it, and how.
    AppPassword {
        url: &'static str,
        label: &'static str,
        steps: &'static [&'static str],
    },
    OAuth2(&'static OAuthConfig),
    /// Not added from the app: the steps say how with the `hysj` command line.
    CommandLine {
        why: &'static str,
        steps: &'static [&'static str],
    },
}

pub struct Provider {
    pub id: &'static str,
    pub name: &'static str,
    /// Domains it serves; `name.*` means any top-level domain (hotmail.no).
    pub domains: &'static [&'static str],
    pub imap: (&'static str, u16),
    /// 465 = TLS from the start, 587 = STARTTLS.
    pub smtp: (&'static str, u16),
    pub auth: Auth,
}

pub const PROVIDERS: &[Provider] = &[
    Provider {
        id: "gmail",
        name: "Gmail",
        domains: &["gmail.com", "googlemail.com"],
        imap: ("imap.gmail.com", 993),
        smtp: ("smtp.gmail.com", 465),
        // Sign in with Google when this build has the client secret; else
        // the command line (app passwords are fragile and may be phased out).
        auth: if GOOGLE_OAUTH.client_secret.is_empty() {
            Auth::CommandLine {
                why: "Hysj doesn’t add Gmail accounts from the app. Google only lets mail apps in with an app password, which takes some setup and may stop working.",
                steps: &[
                    "Turn on 2-Step Verification for your Google account, then create an app password at myaccount.google.com/apppasswords.",
                    "In Terminal, in Hysj’s source code, run: cargo run -p hysj-cli -- account add you@gmail.com — and paste the app password when asked.",
                ],
            }
        } else {
            Auth::OAuth2(&GOOGLE_OAUTH)
        },
    },
    Provider {
        id: "microsoft",
        name: "Outlook.com",
        domains: &[
            "hotmail.*",
            "outlook.*",
            "live.*",
            "msn.com",
            "windowslive.com",
        ],
        imap: ("outlook.office365.com", 993),
        smtp: ("smtp-mail.outlook.com", 587),
        auth: Auth::OAuth2(&MICROSOFT_OAUTH),
    },
    Provider {
        id: "icloud",
        name: "iCloud Mail",
        domains: &["icloud.com", "me.com", "mac.com"],
        imap: ("imap.mail.me.com", 993),
        smtp: ("smtp.mail.me.com", 587),
        auth: Auth::AppPassword {
            url: "https://account.apple.com",
            label: "Apple Account",
            steps: &[
                "Sign in to your Apple Account and open Sign-In and Security.",
                "Choose App-Specific Passwords and create one named “Hysj”.",
                "Paste it below.",
            ],
        },
    },
    Provider {
        id: "yahoo",
        name: "Yahoo Mail",
        domains: &["yahoo.*", "ymail.com", "rocketmail.com"],
        imap: ("imap.mail.yahoo.com", 993),
        smtp: ("smtp.mail.yahoo.com", 465),
        auth: Auth::AppPassword {
            url: "https://login.yahoo.com/account/security",
            label: "Account security",
            steps: &[
                "Open Account security and turn on 2-step verification.",
                "Choose Generate app password and name it “Hysj”.",
                "Paste it below.",
            ],
        },
    },
    Provider {
        id: "fastmail",
        name: "Fastmail",
        domains: &["fastmail.com", "fastmail.fm"],
        imap: ("imap.fastmail.com", 993),
        smtp: ("smtp.fastmail.com", 465),
        auth: Auth::AppPassword {
            url: "https://app.fastmail.com/settings/security/apps",
            label: "App passwords",
            steps: &[
                "Open Settings › Privacy & Security › App passwords.",
                "Make a new one for “Mail (IMAP/POP/SMTP)” named “Hysj”.",
                "Paste it below.",
            ],
        },
    },
];

fn domain(address: &str) -> String {
    address
        .trim()
        .rsplit('@')
        .next()
        .unwrap_or_default()
        .to_lowercase()
}

/// The known provider for an address, if any.
pub fn find(address: &str) -> Option<&'static Provider> {
    let d = domain(address);
    let name = d.split('.').next().unwrap_or_default();
    PROVIDERS.iter().find(|p| {
        p.domains.iter().any(|pat| match pat.strip_suffix(".*") {
            Some(n) => n == name && d.contains('.'),
            None => *pat == d,
        })
    })
}

/// How to sign in an address uses, if it's OAuth.
pub fn oauth_for(address: &str) -> Option<&'static OAuthConfig> {
    match find(address)?.auth {
        Auth::OAuth2(cfg) => Some(cfg),
        _ => None,
    }
}

/// What the add-account form needs to know about an address: the provider,
/// how to sign in and its servers (guessed `imap.`/`smtp.` for others).
pub fn discover(address: &str) -> ProviderInfo {
    let d = domain(address);
    match find(address) {
        Some(p) => {
            let (auth, url, label, steps, with) = match &p.auth {
                Auth::Password => ("password", None, None, Vec::new(), None),
                Auth::AppPassword { url, label, steps } => (
                    "app_password",
                    Some(url.to_string()),
                    Some(label.to_string()),
                    steps.iter().map(|s| s.to_string()).collect(),
                    None,
                ),
                Auth::OAuth2(cfg) => (
                    "oauth",
                    None,
                    None,
                    cfg.notes.iter().map(|s| s.to_string()).collect(),
                    Some(cfg.name.to_string()),
                ),
                Auth::CommandLine { why, steps } => (
                    "command_line",
                    None,
                    Some(why.to_string()),
                    steps.iter().map(|s| s.to_string()).collect(),
                    None,
                ),
            };
            ProviderInfo {
                id: p.id.into(),
                name: p.name.into(),
                known: true,
                auth: auth.into(),
                imap_host: p.imap.0.into(),
                imap_port: p.imap.1,
                smtp_host: p.smtp.0.into(),
                smtp_port: p.smtp.1,
                help_url: url,
                help_label: label,
                help_steps: steps,
                sign_in_with: with,
            }
        }
        None => ProviderInfo {
            id: "generic".into(),
            name: d.clone(),
            known: false,
            auth: "password".into(),
            imap_host: format!("imap.{d}"),
            imap_port: 993,
            smtp_host: format!("smtp.{d}"),
            smtp_port: 465,
            help_url: None,
            help_label: None,
            help_steps: Vec::new(),
            sign_in_with: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn who_an_address_belongs_to() {
        let id = |a: &str| find(a).map(|p| p.id);
        assert_eq!(id("kari@gmail.com"), Some("gmail"));
        assert_eq!(id("Kari@Hotmail.NO"), Some("microsoft"));
        assert_eq!(id("kari@outlook.com"), Some("microsoft"));
        assert_eq!(id("kari@live.no"), Some("microsoft"));
        assert_eq!(id("kari@me.com"), Some("icloud"));
        assert_eq!(id("kari@yahoo.no"), Some("yahoo"));
        assert_eq!(id("kari@fastmail.com"), Some("fastmail"));
        assert_eq!(id("kari@hotmailer.example"), None);
        assert_eq!(id("kari@example.no"), None);
        assert!(oauth_for("kari@hotmail.com").is_some());
        assert_eq!(
            oauth_for("kari@gmail.com").is_some(),
            !GOOGLE_OAUTH.client_secret.is_empty(),
            "Gmail signs in with Google when the build has the secret"
        );
        assert!(oauth_for("kari@example.no").is_none());
    }

    #[test]
    fn unknown_domains_get_guessed_servers() {
        let i = discover("kari@example.no");
        assert_eq!(
            (
                i.known,
                i.auth.as_str(),
                i.imap_host.as_str(),
                i.smtp_host.as_str()
            ),
            (false, "password", "imap.example.no", "smtp.example.no")
        );
        let g = discover("kari@gmail.com");
        let expected = if GOOGLE_OAUTH.client_secret.is_empty() {
            "command_line"
        } else {
            "oauth"
        };
        assert_eq!(
            (g.auth.as_str(), g.imap_host.as_str()),
            (expected, "imap.gmail.com"),
            "Google sign-in with the client secret, else the command line"
        );
        let y = discover("kari@yahoo.no");
        assert_eq!((y.auth.as_str(), y.help_steps.len()), ("app_password", 3));
        let m = discover("kari@hotmail.com");
        assert_eq!(
            (m.auth.as_str(), m.sign_in_with.as_deref()),
            ("oauth", Some("Microsoft"))
        );
    }
}
