# Hysj — a noise-free desktop mail client

Name: **Hysj** (Norwegian for "hush"). The wordmark/logo and splash read **Hysj!**; everywhere else (window title, menus, prose) it is plain **Hysj**, and crates/binaries/bundle IDs use `hysj` — never put `!` in identifiers, paths or commands. A desktop mail client written in Rust whose whole point is
getting rid of noise. Only mail from real people reaches you; everything else is
batched, filed into structured views, or held back until you decide.

This file is the product and architecture brief. Check `git log` for what is
already built; the "Build order" section at the bottom is the original plan.

---

## Product principles

1. **People first.** The main view only ever contains mail from humans you know.
   Newsletters, notifications, receipts and promotions never land there.
2. **Nothing interrupts you except people you know — and live login codes.**
3. **Every sorting decision is explainable.** Each message carries a short
   "why this reached you" reason (e.g. "Asked you a question", "Waiting on you
   for 2 days", "Introduced by Marcus Berg"). If we can't explain it, we
   shouldn't do it.
4. **Local-first and private.** Mail, index and AI all run on the user's machine.
   No cloud services of our own.
5. **When unsure, show the raw thing.** Fall back to the real subject line rather
   than a confidently wrong summary or classification.
6. **One unified inbox** across all accounts, with filtering per account and an
   explicit choice of which address to send from.

---

## Screens (UI reference)

A clickable design canvas exists for all of these (shared separately by Kai —
ask for the link if you need it). Summary:

- **Getting ready (install / bootstrap).** Shown on first launch, before Welcome.
  Headline "Getting Hysj ready." → "Hysj is ready."
  - **On-device AI** card: choose **Standard** (recommended for this Mac; ~2.4 GB),
    **Light** (smaller/faster for older Macs) or **No AI** (rules only — subject
    lines instead of summaries; can be added later in Settings). Recommend a tier
    from RAM / chip. Progress bar with size done, speed, ETA; **Pause/Resume**;
    "Pause on battery and metered connections".
  - **Checklist:** background service installed; passwords in the keychain;
    sorting (embedding) model downloaded + verified; **Notifications → Allow**
    (OS prompt; explain it's only people, codes and signals); **Start at login →
    Turn on** (login item so digests are ready).
  - **Never block:** "Continue while it downloads" goes straight to Welcome;
    summaries switch on when the model is ready.
  Implementation:
  - `hysjd` owns all downloads and reports progress as events (UI and
    `hysj setup` / `hysj models status|pull|remove` show the same thing).
  - A signed/pinned **model manifest** (URL, size, sha256, licence) per tier;
    resumable HTTP range downloads into the models dir; verify the hash, then
    atomic rename; retries with backoff; disk-space check before starting.
  - Model licences: show the model's licence/terms in Settings › On-device AI
    and only mirror weights where the licence allows redistribution (some models,
    e.g. gated ones, need accepting terms — check before relying on a host).
  - Re-runnable from Settings; switching tier downloads the new one before
    deleting the old.
- **Welcome (first run).** One calm flow, same look as the app, no sign-up:
  (Comes after *Getting ready*.)
  1. **Splash:** huge "Hysj!" wordmark, "Only people reach you. Everything else
     waits.", **Get started**, and "Your mail and Hysj's AI stay on this computer."
  2. **Add your mail:** one email field; provider detected from the address
     (Gmail → app-password hint; others → autoconfig). Add several; Continue
     once at least one is connected.
  3. **Digest times:** toggle chips (default 08:00 and 16:00). Explains that
     signals still come through. (Quiet hours were dropped as a feature.)
  4. **Sorting your mail:** real progress from the daemon — emails read,
     people found, held, bookings/orders/codes found, on-device model download
     (show size; the app is usable before the model is ready).
  5. **"It's quiet now."** One sentence with the real counts ("Out of 4,212
     emails, 4 people are waiting on you…"), **Open your Desk**, and two tips:
     ⌘K and "Why here?".
  Step dots in the top strip on steps 2–4; Back on every step. Re-runnable from
  Settings. CLI equivalent: `hysj setup`.
- **Desk (main view) — command-first, no chrome.** No sidebar, no toolbar.
  A slim top strip (wordmark "Hysj!", "41 held for 16:00", an All/Personal/Work
  segmented toggle, "One at a time", "Settings"). Centre column (~760 px):
  - A large **command bar** (⌘K): "Ask or do anything". Typing filters live to
    actions and objects — "Reply to Aisha", "Bergen trip, 6–8 November",
    "Copy sign-in code 482 913", "Where is my USB-C dock?", "Show Work only",
    "Write to Marcus". Suggestion chips under it when empty. Enter runs the top
    hit; nothing matches → full-text search.
  - A sentence headline ("4 people are waiting on you.") and the people as
    borderless rows: avatar with account dot, name, one-sentence gist, wait time
    (accent colour when it's been long).
  - "Also today" as one prose paragraph (introductions, photos, FYIs).
  - Selecting a person opens a **floating conversation panel** on the right:
    chat-style thread (signatures/quotes stripped), suggested replies, From
    picker (defaults to the address it arrived on; flags a different account),
    reply field. Sending clears the person from the Desk.
  - **Bottom dock** (dark pill, centred): People · Digest · Screener · Codes ·
    Bookings · Orders with counts. This replaces the sidebar on every screen.
  - Code pop-up: small floating card bottom-left with the code + Copy.
- **Focus ("one at a time").** One message per screen with big actions:
  Reply (R), Done (E), Snooze (H), Skip (→). Segmented progress in the top strip,
  "Up next" line, and an end state: "That's everyone."
- **Digest — feels like a newspaper, looks like Hysj.** Delivered as editions at
  fixed times (default 08:00 and 16:00). It should *read* like a good paper —
  written in sentences, most important story first, short, with a clear ending —
  but uses the normal app look (Instrument Sans, soft cards, no mastheads/serifs/rules).
  Structure:
  - **Headline + lede** written as sentences: "Nothing here needs you. FINN found
    you a cheaper house." / "41 emails came in while you worked. One code review…"
  - **Stories**, ordered by what matters to the user. Each = small label, a
    one-sentence headline carrying the facts, a short paragraph, then the items.
    E.g. *"FINN reported 4 jobs, 3 houses and 10 cabins for you."* → paragraph
    on what's notable (price drop, viewing) → Houses (cards), Jobs (list),
    Cabins (compact list, "Show 7 more"). Per-story controls ("Pause cabin
    alerts", "Only tell me about price drops").
  - Notifications become prose ("29 notifications, and only one wants something
    from you."), newsletters a short reading list, receipts a sentence with links
    into Bookings/Orders.
  - **Ending:** "That's everything since 08:00. Next digest tomorrow at 08:00",
    plus what was archived unseen and unsubscribe suggestions.
  **Facts in headlines (counts, names, prices, dates) come from extracted data
  via templates** (correct plurals, Norwegian/English) — the LLM may smooth the
  prose and summarise items but is never the source of a number.
  Implementation: per-source extractors (`DigestSource` trait) for high-volume
  senders with stable templates (FINN, airlines, shops, code review/CI), then
  schema.org, then LLM extraction into typed items. De-duplicate items across
  alert emails; track them across editions for *New* / *Price dropped*.
- **Screener.** First-time senders wait here and never notify. One decision per
  sender: **Allow into People / Send to Digest / Block**. Senders skip the
  Screener if you've emailed them before, they're in contacts, or someone you
  trust introduced them. Decisions apply across *all* accounts.
- **Codes.** OTP/sign-in codes and password-reset mails never enter the inbox.
  A code shows as a pop-up (big monospace code + Copy + expiry countdown), then
  self-deletes after expiry. Reset links show whether the link domain matches the
  sender, plus "I didn't ask for this". Option: keep codes on this device only.
- **Bookings.** One card per booking, extracted from confirmation mails; later
  changes are merged into the same card ("Moved from 07:15"). Bookings close in
  time are grouped into a trip. Copyable reference numbers.
  **Add to Wallet** on every booking:
  - If the email already carries a pass (`.pkpass` attachment, or an
    "add to Apple/Google Wallet" link), use that — it's the real one with the
    airline's barcode and live updates. Prefer it always.
  - Otherwise Hysj builds a pass from the extracted booking (title, time,
    place, reference; barcode only if the email contains one — never invent one).
    Apple Wallet: generate a signed `.pkpass` (needs an Apple Developer *Pass
    Type ID* certificate; signing key stays with the daemon/keychain) and hand it
    to the OS to add. Updating passes later needs a pass web service, so v1
    passes are static and re-issued when the booking changes.
  - Google Wallet needs an issuer account and server-side signed JWTs — not
    possible from a purely local app; defer (or only pass through
    airline-provided Google Wallet links).
  - UI label is plain text "Add to Wallet" with a simple wallet icon — do not
    reproduce Apple's or Google's official badge artwork without following their
    badge guidelines.
- **Orders.** One row per order; confirmation + shipping + delivery mails merge
  into a progress tracker (Ordered → Shipped → Out for delivery → Delivered).
  Filters: On the way / Delivered / Return window open.
- **Accounts (settings).** Connected accounts with sync status; add account by
  typing an address. Gmail → app-password flow (with the 3 setup steps); other
  domains → auto-detected IMAP/SMTP settings + password. Sending rules:
  reply from the account it arrived on (default) or the one last used with
  that person; default account for new mail.
- **Compose.** From picker auto-selects the identity you usually use with the
  recipient, and flags when you override it.

### Signals — the important 10% of notifications
Holding notifications for the digest kills most noise; the rest can't wait.
- **Rule: compare the deadline with the next digest.** Extract a deadline or time
  window from each notification. If it falls before the next digest → it's a
  **signal** and shows now; otherwise it's held. ("Domain expires in 30 days" waits;
  "expires tomorrow" comes through.)
- Signal types to recognise (rules first, LLM fallback):
  security (new sign-in, password/2FA changed), money (payment failed, card
  declined, unusual charge, overdue invoice), things you're responsible for
  (CI failing on main, outages, review requested from *you* — use
  `X-GitHub-Reason: review_requested|mention|assign` and equivalents),
  today-only logistics (delivery today, signature needed; flight changes flow
  via Bookings), official mail (tax, government, health).
- **Desk:** a "Can't wait for the digest" section above the people: one compact
  card per signal — title, *why* ("Money · fix it before 16:00 or the order is
  cancelled"), primary action, **Done**, **Hold these for the digest**. Handled or
  expired signals fall back into the digest.
- **Per-source thresholds** instead of all-or-nothing: "CI: only failures on
  main", "GitHub: only when it's me", "Bank: only charges over X".
- Digest items get **Show these right away**; Desk items get **Hold these**.

### Wrong classifications
- **Missing something important is worse than seeing noise.** When unsure, let
  it through. Hard rules that never hold: mail from anyone you've written to,
  replies in threads you're part of, mail asking you a direct question.
- **Every item shows "Why here"** (machine-readable reason → sentence). One-key
  corrections: *Not a person*, *Hold these*, *Let these through* /
  *Should have come through*, *Right call*.
- **A correction becomes a rule immediately**, scoped to sender + type, shown as
  "Learned: … · Undo". It is also stored as a classifier training label, but the
  classifier never overrides a rule the user set.
- **Safety nets:** every digest opens with "Not sure about these N"
  (low-confidence holds, each with *Should have come through* / *Right call*);
  search covers held mail and shows where and why something went; optional
  weekly review of the least-certain decisions.
- **Measure locally:** store each decision with reason + confidence; corrections
  give real precision/recall on the user's own mail. CLI: `hysj why <id>`,
  `hysj misses --week`, `hysj rules` (list/remove learned rules).

### Visual language (CSS design tokens)
Direction: **command-first, no chrome, calm and warm — explicitly not a generic
SaaS dashboard.** No sidebars, no toolbars, no bordered boxes; depth comes from
soft shadows on white surfaces over a warm grey ground. Every screen shares:
slim top strip → centred content column (~760 px) → bottom dock.
Headlines are sentences ("One code is live right now.", "Two parcels on the way.").
- Fonts: **Instrument Sans** (UI), **JetBrains Mono** (times, counts, codes,
  references). Headline 30–34 px / 600 / tight tracking; body 15–17 px.
- Light tokens: ground `#F2F2EF`, surface `#FFFFFF`, subtle fill `#E6E6E2`,
  hairline `#EDEDEA`, ink `#111111`, body `#2A2C30`, secondary `#55575C`,
  muted `#6A6C71`, accent `#C83A14` (vermilion — used sparingly: long waits,
  live codes, "arrives today"). Primary buttons are **ink** (`#111111` bg, white
  text), secondary buttons are subtle fills, no outlines.
  Status chips: positive `#DCEBE3/#1F5A40`, warning `#F6E3D0/#8A4B0F`,
  neutral `#E6E6E2/#2A2C30`, danger text `#B42318`.
  Account dots: Personal `#E38B3A`, Work `#2E7D5B`.
- Surfaces: radius 16–24, shadow `0 8px 24px rgba(17,17,17,.06)`; floating
  panels `0 24px 60px rgba(17,17,17,.16)`. Dock: `rgba(17,17,17,.92)` pill.
- **Dark mode is required**, follows the OS by default with a manual override.
  Dark tokens: ground `#141413`, surface `#1E1E1C`, subtle `#2A2A27`,
  ink `#F2F2EF`, body `#D6D6D2`, secondary `#A9A9A3`, muted `#8F8F89`,
  accent `#FF7A4D`; primary buttons invert (`#F2F2EF` bg, `#141413` text);
  the dock inverts to a light pill. Account dots: `#F0A35E` / `#4FB386`.
  Build both from one token set — no widget hard-codes a colour.
- Touch targets ≥ 44 px. Text contrast ≥ 4.5:1. No emoji; simple stroke icons.
- Implement the tokens as CSS custom properties; light/dark via
  `prefers-color-scheme` plus a `data-theme` override. Shadows, radii and the
  dock are plain CSS. `backdrop-filter` is available now but the design doesn't
  rely on it. Bundle the fonts locally — no Google Fonts or other network
  requests at runtime.

---

## Architecture

Split into a background **daemon** that owns everything, and thin clients.

```
crates/
  hysj-core    models, SQLite store, classification pipeline, scheduler logic,
                MailBackend trait. No UI, no networking specifics.
  hysj-imap    IMAP + SMTP backend; Gmail specifics as a sub-module.
  hysj-ai      embeddings, classifier, local LLM (behind traits).
  hysj-proto   API types shared by daemon and clients (serde), version handshake.
  hysjd        the daemon: sync, AI, scheduler (digests), OS notifications.
  hysj-app     Tauri 2 desktop client: Rust side is a hysjd client; web UI in
                crates/hysj-app/ui.
  hysj-cli     CLI client (`hysj …`).
```

Why a daemon: the digest must be assembled at 16:00, code pop-ups must fire on
arrival, and sync/AI should run while idle — whether or not a window is open.
It also isolates the model (an OOM can't kill the UI) and gives us a CLI for free.

### Daemon rules
- **Only `hysjd` touches the store, the IMAP connections and the models.**
  Clients hold a view cache only.
- **Transport:** Unix domain socket (macOS/Linux), named pipe (Windows), owner-only
  permissions. **No TCP listener** — other local processes must not be able to
  read mail. Remote access, if ever, is a separate deliberate feature with TLS+auth.
- **Protocol:** JSON-RPC 2.0 over the socket, types from `hysj-proto`.
  Version handshake on connect.
- **Sync model (JMAP-style):** the daemon pushes small "state changed" events
  (e.g. `{ "type": "people", "state": 812 }`); clients call `*/changes` since
  their last state and fetch deltas. Never stream whole mailboxes.
- **Pluggable transport:** a `Transport` trait with a socket implementation and an
  in-process channel implementation. Tests and early development run everything
  in one process.
- **Lifecycle:** the app starts `hysjd` if it isn't running; optional login item
  (launchd / systemd user service). When no UI is connected the daemon posts OS
  notifications itself (e.g. codes).

### CLI
Thin client over the same API. Useful for scripting and for testing the backend
without the GUI, e.g.:
```
hysj sync [--account work]
hysj list --bucket people|digest|screener [--account …] [--json]
hysj show <id>
hysj screener allow|digest|block <sender>
hysj codes --latest          # prints the current live code
hysj send --from work --to … --subject … < body.txt
```
Prefer `--json` output on every command so it's scriptable and testable.

### Desktop client (Tauri 2)
The app moved from egui to **Tauri 2** (the egui version is in git history).
- **The Rust side owns the daemon connection.** It is a normal `hysjd` client
  (`Transport` + `hysj-proto`), exposes typed `#[tauri::command]`s to the UI and
  forwards the daemon's state-changed events as Tauri events. The webview never
  touches the socket, the store, the keychain or the filesystem.
- **Shared types:** generate TypeScript types from `hysj-proto` (e.g. `specta` /
  `tauri-specta` or `ts-rs`) so UI and daemon can't drift.
- **Lock the webview down:** minimal Tauri capabilities (no fs/shell/http
  plugins exposed to the UI), strict CSP, no remote scripts or fonts.
- **Rendering mail HTML is the main security risk.** Sanitize on the Rust side
  (e.g. `ammonia`), render bodies in a sandboxed iframe without
  `allow-scripts`, and **block remote images by default** (tracking pixels) with
  a per-sender "load images". Links open in the system browser.
- **Window:** no native chrome — the top strip is the title bar
  (macOS `titleBarStyle: "Overlay"` + a drag region; keep traffic lights clear).
- **Keyboard first:** ⌘K opens the command bar from anywhere in the window;
  every action reachable without the mouse.
- **Accessibility:** semantic HTML (`button`, `a`, `label`/`input`), visible focus,
  `aria-label` on icon-only controls.
- The design mockups are HTML; their layout and tokens can be ported almost
  directly into components.
- Frontend framework: whatever the UI uses (keep it small); no UI kit — the
  look is custom.

### Auto-update (GitHub Releases)
- Use the **Tauri 2 updater plugin** with a static JSON endpoint on GitHub
  Releases: `https://github.com/<owner>/<repo>/releases/latest/download/latest.json`.
  CI (`tauri-apps/tauri-action`) builds, signs and uploads the bundles plus
  `latest.json` on every tagged release.
- **Two kinds of signing, both required:**
  - Tauri updater signature (minisign key pair; public key in
    `tauri.conf.json`, private key + password only as CI secrets). The app
    refuses unsigned or wrongly signed updates.
  - OS code signing: macOS Developer ID + notarization (otherwise Gatekeeper
    blocks the app and updates); Windows Authenticode when Windows ships.
- **The release repo must be public** (or use a separate public releases repo):
  the updater can't authenticate to download private release assets.
- **Daemon goes with the app:** ship `hysjd` (and `hysj` CLI) as Tauri
  sidecars inside the bundle, so one update replaces all three. After install
  the app restarts; `hysjd` notices the new build and hands over (existing
  "exit when rebuilt" logic). The `hysj-proto` version handshake must reject
  mismatched client/daemon versions gracefully.
- **Data safety:** the daemon runs SQLite schema migrations on start, after
  taking a backup; migrations are forward-only and tested on fixtures. Never
  auto-downgrade.
- **Models are not in the app bundle** — they update separately via the model
  manifest (see Getting ready).
- **Channels:** stable uses `releases/latest`. A beta channel can't use
  "latest" (pre-releases are skipped), so point beta at a fixed endpoint, e.g.
  a `beta.json` asset on a rolling `beta-channel` release or GitHub Pages.
- **UX — never interrupt:** check on launch and every few hours (jittered),
  download quietly, then show a small "Update ready · Restart" pill in the top
  strip; otherwise install on next quit. Release notes written as one or two
  sentences. Settings › About: version, channel, "Check now", turn automatic
  checks off (an update check reveals IP + version to GitHub — say so).
  CLI: `hysj update [--check]`.

---

## Mail backends

### Accounts vs identities
An **account** is a server connection; an **identity** is a send-from address
(an account can have several, e.g. aliases). Replies default to the identity the
message arrived on; new mail defaults per settings; Compose suggests the identity
most recently used with that recipient (derived from Sent).

### `MailBackend` trait (sketch)
`sync_since(cursor) -> Changes`, `fetch(ids, parts)`, `apply(changes)`
(flags/labels/move/delete), `send(msg, identity)`.

### Generic IMAP + SMTP (v1, used for *all* accounts incl. Gmail)
- Reading: `async-imap` (or `imap-codec` if we need more control).
  Use CONDSTORE (and QRESYNC where supported) for change sync; IDLE for push.
- Sending: `lettre`. Parsing: `mail-parser`. DKIM/SPF/DMARC: `mail-auth`.
- Secrets: `keyring` (system keychain). Never on disk in our files, never logged.
- Autodiscovery: Thunderbird ISPDB / provider autoconfig → RFC 6186 SRV records
  → guess from MX.

### Gmail over IMAP (v1 decision)
- Auth: **app password** (requires 2-Step Verification). Google OAuth for Gmail is
  a restricted scope (verification + security assessment to distribute), so it's
  deferred; keep auth pluggable. App passwords may be phased out — don't assume.
- Sync **only `[Gmail]/All Mail`**; read labels with `X-GM-LABELS`, IDs with
  `X-GM-MSGID`, threads with `X-GM-THRID`. Labels become tags; never duplicate.
- CONDSTORE yes, QRESYNC no → detect deletions by periodic UID-set comparison.
- Re-issue IDLE every few minutes; reconnect quietly.
- Archive = remove `\Inbox` label; delete = move to `[Gmail]/Trash`.
- Gmail's SMTP saves to Sent automatically — **do not APPEND** to Sent.
- Respect connection and bandwidth limits: headers first, bodies lazily.

### Later
JMAP backend (`jmap-client`), Gmail API backend if OAuth becomes worthwhile.

### Cross-account behaviour
One SQLite store with `account_id` columns. Dedupe the same message delivered to
two accounts via `Message-ID`. Contacts/sender decisions are global.

---

## Classification pipeline (cheapest first)

1. **Deterministic rules** (handles most volume):
   - Bulk signals: `List-Unsubscribe`, `Precedence: bulk/list`, `Auto-Submitted`.
   - Relationship: have I ever sent to this address? In contacts? Introduced by
     a known contact (they were CC'd / named)?
   - **schema.org** JSON-LD/microdata in HTML: `FlightReservation`,
     `LodgingReservation`, `FoodEstablishmentReservation`, `Order`,
     `ParcelDelivery` → typed Bookings/Orders with no model at all.
   - **Codes:** 4–8 digit code near keywords (code/kode/verification…) from a
     DKIM-aligned sender; reset links with domain check.
2. **Embeddings + small classifier** on every message: multilingual small
   embedding model (e5-small / BGE-small class) via `fastembed`/`ort`.
   Buckets: Reply / Read / Know, person vs bulk. User corrections in the UI are
   training labels. Same embeddings power semantic search.
3. **Generative LLM only for what reaches the user** (tens per day):
   one-sentence gists, suggested replies, extraction for bookings/orders without
   schema.org, quote/signature stripping where rules fail.

Every decision stores a machine-readable *reason* so the UI can show "why".

## Local AI
- Runtime behind a trait. Start with **mistral.rs**; keep **llama.cpp**
  (`llama-cpp-2`) as fallback. `ort` for embeddings/classifier.
- Model: 1–4B instruct, 4-bit quantised, **strongly multilingual** (mail is
  English + Norwegian). Benchmark candidates (e.g. Qwen3 / Gemma 3 small
  sizes, or newer) on a sample of real mail before committing.
- Download on first run; don't bundle.
- **Constrained output:** JSON schema / grammar → deserialize straight into Rust
  structs with serde.
- Cache results in SQLite keyed by `(message_id, model_version, task)`.
  Backfill when idle / on power.
- Low confidence → show raw subject instead of a gist.

---

## Data model (sketch)
`accounts`, `identities`, `messages` (headers, account, flags, tags, bucket,
reason), `bodies` (lazy), `threads`, `contacts`, `sender_decisions`
(allow/digest/block, global), `codes` (with expiry; optionally never synced), `signals` (deadline, state),
`rules` (user-created/learned, scope sender+type, action), `decisions` (bucket,
reason, confidence, model version), `corrections`,
`bookings`, `orders` (+ events), `ai_cache`, `embeddings`, `sync_state` per
account/mailbox.

---

## Conventions
- Rust stable, edition 2024, Cargo workspace. `cargo fmt`, `cargo clippy -- -D warnings`.
- `tracing` for logs. **Never log message bodies, addresses in bulk, or secrets.**
- Errors: `thiserror` in libraries, `anyhow` in binaries.
- Tests: fixture `.eml` files for parsing/classification; IMAP integration tests
  against a local test server in Docker (e.g. GreenMail or Stalwart).
- Keep `hysj-core` free of UI and network-library types.
- Frontend: typed (TypeScript), formatted and linted in CI; no network access at runtime.
- Small, focused commits.

---

## Build order
Now: **port `hysj-app` from egui to Tauri 2** at feature parity (Desk with
command bar + conversation panel, Focus, Digest, Screener, Codes, Bookings,
Orders, Settings, code pop-up, dark mode), then continue below.

1. **Workspace skeleton**: all crates, `hysj-proto` types, in-process transport,
   `hysj-cli` talking to an in-process daemon.
2. **IMAP sync** of one generic account → SQLite; `hysj list` / `hysj show`.
3. **Gmail specifics** + second account; unified store; dedupe; identities.
4. **Rules tier**: People / Screener / Digest buckets with reasons; Screener
   decisions via CLI.
5. **Desktop client** (now Tauri 2): Desk, Screener, Digest, account filter, design tokens.
6. **Sending**: SMTP via `lettre`, identity selection, Compose.
7. **Codes, Bookings, Orders** (rules + schema.org first).
8. **Daemon as a real process**: socket transport, lifecycle, digest scheduler,
   OS notifications.
9. **AI tiers**: embeddings + classifier, then LLM gists and suggested replies.
10. Focus mode, keyboard shortcuts, polish.

## Open questions
- Windows support from day one, or macOS/Linux first?
- When (if) to add Google OAuth / Gmail API and JMAP.
