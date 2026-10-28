//! `hysj` — thin CLI client over the daemon API.

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use hysj_core::Client;
use hysj_proto::{
    Account, AccountAddParams, Bucket, Decision, Identity, ListParams, Message, MessageDone,
    MessageSummary, ScreenerDecide, ScreenerDecided, ScreenerForget, SendParams, SendResult,
    ShowParams, SyncParams, SyncResult, method,
};

#[derive(Parser)]
#[command(name = "hysj", about = "Hysj mail, from the terminal")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
    /// Print JSON instead of text.
    #[arg(long, global = true)]
    json: bool,
}

#[derive(Subcommand)]
enum Cmd {
    /// List messages in a bucket.
    List {
        #[arg(long, value_enum)]
        bucket: BucketArg,
        #[arg(long)]
        account: Option<String>,
    },
    /// Show one message.
    Show { id: String },
    /// Ask a question about your mail, answered on this computer.
    Ask { question: Vec<String> },
    /// Fetch new mail.
    Sync {
        #[arg(long)]
        account: Option<String>,
    },
    /// Manage accounts.
    #[command(subcommand)]
    Account(AccountCmd),
    /// Decide where a sender's mail goes, across all accounts.
    Screener {
        #[arg(value_enum)]
        decision: DecisionArg,
        /// Sender address.
        sender: String,
    },
    /// Send mail; the body is read from stdin.
    Send {
        /// One of your addresses (see `hysj account list`).
        #[arg(long)]
        from: String,
        #[arg(long, required_unless_present = "reply_to")]
        to: Vec<String>,
        #[arg(long)]
        cc: Vec<String>,
        /// Defaults to "Re: …" when replying.
        #[arg(long, default_value = "")]
        subject: String,
        /// Reply to this message id (threading, recipient, subject).
        #[arg(long)]
        reply_to: Option<String>,
    },
    /// Local AI tools.
    #[command(subcommand)]
    Ai(AiCmd),
    /// Clear messages from the Desk / Digest.
    Done {
        #[arg(required = true)]
        ids: Vec<String>,
    },
}

#[derive(Subcommand)]
enum AiCmd {
    /// Run a model on a sample of your mail; writes outputs and timings as JSON.
    Bench {
        /// qwen3 or gemma3
        #[arg(long)]
        model: String,
        #[arg(long, default_value_t = 20)]
        sample: usize,
        #[arg(long)]
        out: std::path::PathBuf,
    },
}

#[derive(Subcommand)]
enum AccountCmd {
    /// Connect an account; prompts for the (app) password.
    Add {
        address: String,
        /// Short name for `--account` filters [default: the address].
        #[arg(long)]
        name: Option<String>,
        /// IMAP host [default: guessed from the address].
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        port: Option<u16>,
        /// Login name [default: the address].
        #[arg(long)]
        username: Option<String>,
        /// SMTP host [default: guessed from the address].
        #[arg(long)]
        smtp_host: Option<String>,
        /// 465 (TLS) or 587 (STARTTLS) [default: 465].
        #[arg(long)]
        smtp_port: Option<u16>,
    },
    List,
}

#[derive(Clone, Copy, ValueEnum)]
enum BucketArg {
    People,
    Digest,
    Screener,
    Blocked,
    Codes,
}

impl From<BucketArg> for Bucket {
    fn from(b: BucketArg) -> Self {
        match b {
            BucketArg::People => Bucket::People,
            BucketArg::Digest => Bucket::Digest,
            BucketArg::Screener => Bucket::Screener,
            BucketArg::Blocked => Bucket::Blocked,
            BucketArg::Codes => Bucket::Codes,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum DecisionArg {
    /// Into People.
    Allow,
    /// Into the Digest.
    Digest,
    /// Never shown again.
    Block,
    /// Undo your decision.
    Forget,
}

fn main() -> Result<()> {
    // Before clap and before a runtime: `hysj --daemon` is the daemon.
    hysjd::run_if_daemon();
    hysjd::set_models_dir();
    tokio::runtime::Runtime::new()?.block_on(cli_main())
}

async fn cli_main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();

    let mut client = hysjd::socket::client(Some("cli")).await?;

    match cli.cmd {
        Cmd::List { bucket, account } => {
            let list: Vec<MessageSummary> = client
                .call(
                    method::LIST,
                    ListParams {
                        bucket: bucket.into(),
                        account,
                    },
                )
                .await?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&list)?);
            } else {
                for m in &list {
                    let date = m.date.as_deref().map_or("", |d| &d[..10]);
                    println!(
                        "{:>5}  {date:<10}  {:<16}  {:<24}  {:<50}  {}",
                        m.id,
                        clip(&m.account, 16),
                        clip(&m.from, 24),
                        clip(&m.subject, 50),
                        m.reason.text()
                    );
                }
            }
        }
        Cmd::Show { id } => {
            let m: Message = client.call(method::SHOW, ShowParams { id }).await?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&m)?);
            } else {
                let s = &m.summary;
                println!(
                    "From:    {} <{}>\nDate:    {}\nAccount: {}\nSubject: {}\nWhy:     {}\n\n{}",
                    s.from,
                    s.from_addr,
                    s.date.as_deref().unwrap_or("-"),
                    s.account,
                    s.subject,
                    s.reason.text(),
                    m.body
                );
            }
        }
        Cmd::Ask { question } => {
            let a: hysj_proto::Asked = client
                .call(
                    method::ASK,
                    hysj_proto::AskParams {
                        question: question.join(" "),
                    },
                )
                .await?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&a)?);
            } else if let Some(a) = &a.answer {
                println!("{}\n", a.text);
                for s in &a.sources {
                    println!("  {}  {} · {}", s.id, s.from, s.subject);
                }
                println!("\n{}", a.caveat);
            } else {
                println!("{}", a.note);
            }
        }
        Cmd::Sync { account } => {
            let results: Vec<SyncResult> =
                client.call(method::SYNC, SyncParams { account }).await?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&results)?);
            } else {
                for r in &results {
                    println!("{}: {} new", r.account, r.new);
                }
            }
        }
        Cmd::Account(AccountCmd::Add {
            address,
            name,
            host,
            port,
            username,
            smtp_host,
            smtp_port,
        }) => {
            let password = read_password(&address)?;
            let params = AccountAddParams {
                name: name.unwrap_or_else(|| address.clone()),
                address,
                host,
                port,
                username,
                smtp_host,
                smtp_port,
                password,
            };
            let a: Account = client.call(method::ACCOUNT_ADD, params).await?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&a)?);
            } else {
                println!(
                    "Added {} ({} via {}:{}). Run `hysj sync` next.",
                    a.name, a.address, a.host, a.port
                );
            }
        }
        Cmd::Screener { decision, sender } => {
            let decision = match decision {
                DecisionArg::Allow => Decision::Allow,
                DecisionArg::Digest => Decision::Digest,
                DecisionArg::Block => Decision::Block,
                DecisionArg::Forget => {
                    let n: usize = client
                        .call(
                            method::SCREENER_FORGET,
                            ScreenerForget {
                                sender: sender.clone(),
                            },
                        )
                        .await?;
                    println!("{sender}: decision removed, {n} message(s) re-sorted");
                    return Ok(());
                }
            };
            let r: ScreenerDecided = client
                .call(method::SCREENER_DECIDE, ScreenerDecide { sender, decision })
                .await?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&r)?);
            } else {
                let bucket = serde_json::to_value(r.bucket)?;
                println!(
                    "{}: {} message(s) now in {}",
                    r.sender,
                    r.messages,
                    bucket.as_str().unwrap_or("?")
                );
            }
        }
        Cmd::Send {
            from,
            to,
            cc,
            subject,
            reply_to,
        } => {
            let mut body = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut body)?;
            let params = SendParams {
                from,
                to,
                cc,
                subject,
                body,
                reply_to,
            };
            let r: SendResult = client.call(method::SEND, params).await?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&r)?);
            } else {
                println!("Sent <{}>", r.message_id);
            }
        }
        Cmd::Ai(AiCmd::Bench { model, sample, out }) => {
            bench(&mut client, &model, sample, &out).await?;
        }
        Cmd::Done { ids } => {
            let n: usize = client
                .call(method::MESSAGE_DONE, MessageDone { ids })
                .await?;
            println!("{n} message(s) cleared");
        }
        Cmd::Account(AccountCmd::List) => {
            let accounts: Vec<Account> = client.call(method::ACCOUNT_LIST, ()).await?;
            let identities: Vec<Identity> = client.call(method::IDENTITY_LIST, ()).await?;
            if cli.json {
                let out = serde_json::json!({ "accounts": accounts, "identities": identities });
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                for a in &accounts {
                    println!("{:<16} {:<32} {}:{}", a.name, a.address, a.host, a.port);
                    for i in identities.iter().filter(|i| i.account == a.name) {
                        println!("  sends as {}", i.address);
                    }
                }
            }
        }
    }
    Ok(())
}

/// Prompts on the terminal; without one (scripts, pipes) reads one line from stdin,
/// e.g. `pbpaste | hysj account add …`.
fn read_password(address: &str) -> Result<String> {
    use std::io::IsTerminal;
    if std::io::stdin().is_terminal() {
        return Ok(rpassword::prompt_password(format!(
            "Password for {address}: "
        ))?);
    }
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let password = line.trim_end_matches(['\r', '\n']).to_string();
    anyhow::ensure!(!password.is_empty(), "no password on stdin");
    Ok(password)
}

fn clip(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

/// Gists (all), replies (mail from people) and one Digest summary, on the same
/// sample for every model. Bodies come through the daemon (fetched and cached).
async fn bench(
    client: &mut Client<hysjd::socket::SocketTransport>,
    model: &str,
    sample: usize,
    out: &std::path::Path,
) -> Result<()> {
    use hysj_ai::{Choice, Llm, MessageText, timed};
    let choice = Choice::parse(model).ok_or_else(|| anyhow::anyhow!("--model qwen3 or gemma3"))?;
    let digest: Vec<MessageSummary> = client
        .call(
            method::LIST,
            ListParams {
                bucket: Bucket::Digest,
                account: None,
            },
        )
        .await?;
    let done: Vec<MessageSummary> = client
        .call(
            method::FOLDER_LIST,
            hysj_proto::FolderParams {
                folder: hysj_proto::Folder::Done,
                account: None,
            },
        )
        .await?;
    // People: cleared mail that wasn't bulk; the rest from the Digest.
    let people: Vec<&MessageSummary> = done
        .iter()
        .filter(|m| {
            matches!(
                m.reason,
                hysj_proto::Reason::EmailedBefore { .. }
                    | hysj_proto::Reason::Introduced { .. }
                    | hysj_proto::Reason::FirstTime
            )
        })
        .take(sample / 4)
        .collect();
    let picked: Vec<&MessageSummary> = people
        .iter()
        .copied()
        .chain(digest.iter().take(sample - people.len()))
        .collect();

    let (llm, load) = timed(Llm::load(choice)).await;
    let llm = llm?;
    let mut rows = Vec::new();
    for m in &picked {
        let body: Message = client
            .call(method::SHOW, ShowParams { id: m.id.clone() })
            .await?;
        let text = MessageText {
            from: &m.from,
            subject: &m.subject,
            body: &body.body,
        };
        let (gist, gist_s) = timed(llm.gist(&text)).await;
        let person = people.iter().any(|p| p.id == m.id);
        let replies = if person {
            Some(timed(llm.replies(&text)).await)
        } else {
            None
        };
        rows.push(serde_json::json!({
            "from": m.from, "subject": m.subject, "person": person,
            "gist": gist.as_ref().map(|g| serde_json::json!(g)).unwrap_or_else(|e| serde_json::json!({"error": e.to_string()})),
            "gist_s": gist_s,
            "replies": replies.as_ref().map(|(r, _)| r.as_ref().map(|r| serde_json::json!(r)).unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}))),
            "replies_s": replies.as_ref().map(|(_, t)| *t),
        }));
    }
    let edition: hysj_proto::Edition = client
        .call(
            method::DIGEST_EDITION,
            hysj_proto::EditionParams { pick: 0 },
        )
        .await?;
    let facts = hysj_core::edition::facts(&edition);
    let (summary, digest_s) = timed(llm.digest(&facts)).await;
    let report = serde_json::json!({
        "model": choice.version(), "load_s": load, "rows": rows,
        "digest": summary.map(|d| serde_json::json!(d)).unwrap_or_else(|e| serde_json::json!({"error": e.to_string()})),
        "digest_s": digest_s, "digest_facts": facts,
    });
    std::fs::write(out, serde_json::to_string_pretty(&report)?)?;
    println!("wrote {} rows to {}", picked.len(), out.display());
    Ok(())
}
