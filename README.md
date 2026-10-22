# Hysj

A noise-free desktop mail client. Only mail from people you know reaches you;
newsletters, notifications and receipts wait for a digest at the times you
choose, and codes, bookings and orders are pulled out into their own places.
Everything — your mail, the index and the AI — stays on your computer.

Hysj is Norwegian for "hush".

## Status

Early and personal. It runs on **Macs with Apple Silicon** (M1 or newer).
There are no signed builds yet, so you build it yourself.

## Build and run

You need [Rust](https://rustup.rs) (stable) and [Node.js](https://nodejs.org) 20 or newer.

```sh
git clone https://github.com/thrandre/hysj
cd hysj
cargo run --release
```

The first run asks you to add your mail:

- **Gmail:** turn on 2-Step Verification, then create an
  [app password](https://myaccount.google.com/apppasswords) for Hysj.
- **Other providers:** your normal password; Hysj guesses the IMAP/SMTP
  servers from the address, or you type them in.

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

## License

[MIT](LICENSE)
