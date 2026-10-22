# Hysj

A noise-free desktop mail client. Only mail from people you know reaches you;
newsletters, notifications and receipts wait for a digest at the times you
choose, and codes, bookings and orders are pulled out into their own places.
Everything — your mail, the index and the AI — stays on your computer.

Hysj is Norwegian for "hush".

## Status

Early and personal. It runs on **Macs with Apple Silicon** (M1 or newer).

## Install

1. Download the `.dmg` from the [latest release](https://github.com/thrandre/hysj/releases/latest)
   and drag Hysj to Applications.
2. The builds aren't signed by Apple yet, so macOS blocks the first launch.
   Open **System Settings › Privacy & Security**, scroll down and choose
   **Open Anyway** next to Hysj.
3. After that Hysj updates itself: new versions download in the background
   and install when you restart or quit it. macOS asks for keychain access
   again after each update — choose **Always Allow**.

## Build and run

You need [Rust](https://rustup.rs) (stable) and [Node.js](https://nodejs.org) 20 or newer.

```sh
git clone https://github.com/thrandre/hysj
cd hysj
cargo run --release
```

The first run asks you to add your mail:

- **Gmail:** Sign in with Google (in your browser). Google warns that it
  hasn't verified Hysj: choose **Advanced › Go to Hysj**. (Hysj's Google
  registration is unverified, which Google allows for up to 100 people.)
- **Outlook.com / Hotmail:** Sign in with Microsoft (in your browser).
- **iCloud, Yahoo, Fastmail:** an app password; Hysj shows where to make it.
- **Other providers:** your normal password; Hysj guesses the IMAP/SMTP
  servers from the address, or you type them in.

Building from source: Google requires a client secret even for desktop apps,
which isn't in the repo. Set `HYSJ_GOOGLE_CLIENT_SECRET` when building (or,
on a Mac, keep it in the keychain item `hysj-google-client-secret`); without
it, Gmail is added with the command line instead:
`cargo run -p hysj-cli -- account add you@gmail.com` and an
[app password](https://myaccount.google.com/apppasswords).

Passwords go in the macOS keychain. macOS asks whether Hysj may read it after
every new build — choose **Always Allow**.

The on-device AI (Gemma 3 4B, about 4 GB) downloads the first time it's needed.
On a Mac with 8 GB of memory, or to skip it, turn it off in
Settings › On-device AI; sorting still works without it.

## How it's built

A background service (`hysjd`) owns the mail store, the IMAP/SMTP connections
and the model; the desktop app (Tauri 2 + Svelte) and the `hysj` command line
are thin clients over a local socket. See [CLAUDE.md](CLAUDE.md) for the full
product and architecture brief.

```sh
cargo test --workspace           # Rust tests
cd crates/hysj-app/ui && npm test   # UI tests
```

## Releasing

1. Bump `version` in the root `Cargo.toml` and commit.
2. Tag with the release notes as the message (after a title line; they're
   what installed copies show as "what's new") and push:
   `git tag -a v0.3.0 -m "Hysj 0.3.0" -m "What changed, in a sentence or two."`
   then `git push origin v0.3.0`.

The Release workflow builds the app on an Apple Silicon runner, publishes the
`.dmg`, and the update that installed copies pick up (`latest.json`, signed
with the updater key in the repository secrets). A local build:
`cargo install tauri-cli --version "^2"`, then `cargo tauri build` in
`crates/hysj-app`.

## License

[MIT](LICENSE)
