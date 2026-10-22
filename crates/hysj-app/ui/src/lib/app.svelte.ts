// App state: the daemon's snapshot plus what the reader is looking at.

import { api, onChanged, onUpdateReady, snapshot } from './api.ts';
import type { MessageSummary, MessageView, Snapshot, UpdateStatus } from './types.ts';
import type { Go } from './hits.ts';
import { lastSlot, setDigestHours } from './words.ts';

export type View = 'desk' | 'focus' | 'digest' | 'screener' | 'codes' | 'bookings' | 'orders' | 'sent' | 'done' | 'settings' | 'search';

export const app = $state({
  data: null as Snapshot | null,
  view: (new URLSearchParams(location.search).get('view') ?? 'desk') as View,
  /** Desk account filter (account name). */
  account: null as string | null,
  /** The open message (Desk panel, or in place elsewhere). */
  open: null as string | null,
  error: null as string | null,
  busy: null as string | null,
  /** Rendered messages by id (fetched when opened). */
  views: {} as Record<string, MessageView>,
  /** A new email being written (Compose), with its recipient if known. */
  compose: null as { to: string } | null,
  /** Replies being written, by message id. */
  drafts: {} as Record<string, { from: string; text: string; sending: boolean }>,
  /** Which Copy button last copied ("code:…", "ref:…"). */
  copied: null as string | null,
  /** What "Search all mail" looks for. */
  searchQuery: '',
  /** ⌘K asks the command bar to take focus. */
  focusCommand: 0,
  /** Digest delivery shown: 0 latest, 1–2 earlier, 3 older. */
  digestPick: 0,
  /** Orders filter: 0 on the way, 1 delivered, 2 returns. */
  orderTab: 0,
  showPast: false,
  /** Screener decisions this session, shown with Undo: [decision, name, subject]. */
  decided: [] as { sender: string; bucket: string; name: string; subject: string }[],
  /** The app's update state ("Update ready" in the top strip). */
  update: null as UpdateStatus | null,
  /** Settings' open tab (the update pill opens About). */
  settingsTab: 'accounts' as 'accounts' | 'digest' | 'ai' | 'appearance' | 'about',
  /** The first-run Welcome's step (0–4), or null once the Desk is open. */
  welcome: (new URLSearchParams(location.search).has('welcome') ? 0 : null) as number | null,
  /** What Hysj just learned from a correction ("Learned: …"), with Undo. */
  learned: null as { text: string; undo: () => Promise<unknown> } | null,
  /** Focus: the session's queue of message ids, and where we are. */
  focus: { queue: [] as string[], at: 0, replying: false },
  /** The code in the pop-up, and every code already popped up this session. */
  popup: null as string | null,
  popupShown: [] as string[],
  theme: (localStorage.getItem('hysj-theme') ?? 'system') as 'system' | 'light' | 'dark',
});

/** Light, dark or the system's; remembered on this computer. */
export function setTheme(t: 'system' | 'light' | 'dark'): void {
  app.theme = t;
  try {
    localStorage.setItem('hysj-theme', t);
  } catch {
    // Private storage off: the choice lasts for this session.
  }
  if (t === 'system') delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = t;
}
setTheme(app.theme);

export async function copy(text: string, key: string): Promise<void> {
  await navigator.clipboard.writeText(text);
  app.copied = key;
}

/** Runs what a command-bar result does. */
export function perform(go: Go): void {
  if ('open' in go) {
    go_(go.view);
    toggle(go.open);
  } else if ('view' in go) go_(go.view);
  else if ('copy' in go) void copy(go.copy, go.key);
  else if ('compose' in go) app.compose = { to: go.compose };
  else if ('search' in go) {
    app.searchQuery = go.search;
    go_('search');
  } else if ('sync' in go) void act('Checking for new mail…', () => api.sync());
  else if ('account' in go) {
    go_('desk');
    app.account = go.account;
  }
}

/** Opens a message: the Desk's panel, or in place elsewhere. */
export function toggle(id: string): void {
  if (app.open === id) {
    app.open = null;
    return;
  }
  app.open = id;
  if (!app.views[id]) {
    api
      .messageView(id)
      .then((v) => {
        app.views[id] = v;
      })
      .catch((e) => {
        app.error = String(e);
      });
  }
}

let loading = false;
let again = false;

/** Fetches everything; coalesces bursts of change events into one reload. */
export async function reload(): Promise<void> {
  if (loading) {
    again = true;
    return;
  }
  loading = true;
  try {
    const first = !app.data;
    const data = await snapshot();
    setDigestHours(data.digestHours);
    app.data = data;
    app.error = null;
    if (first && app.view === 'focus') startFocus();
    // No mail yet: the first run starts with the Welcome.
    if (first && data.accounts.length === 0) app.welcome = 0;
    // Pop up the newest live code not shown yet this session.
    const now = Date.now() / 1000;
    const fresh = app.data.codes.find((c) => !c.reset && c.expires > now && !app.popupShown.includes(c.id));
    if (app.data.codeSettings.popup && fresh) {
      app.popupShown.push(fresh.id);
      app.popup = fresh.id;
    }
  } catch (e) {
    app.error = String(e);
  } finally {
    loading = false;
    if (again) {
      again = false;
      void reload();
    }
  }
}

export function start(): void {
  void reload();
  void onChanged(() => void reload());
  const update = () => void api.updateStatus().then((s) => (app.update = s), () => {});
  update();
  void onUpdateReady(update);
}

/** Runs an action with a busy note; reloads after, errors land in the banner. */
export async function act<T>(busy: string | null, f: () => Promise<T>): Promise<T | undefined> {
  app.busy = busy;
  try {
    const r = await f();
    await reload();
    return r;
  } catch (e) {
    app.error = String(e);
    return undefined;
  } finally {
    app.busy = null;
  }
}

/** Sends a sender's mail to the Digest from now on. Undo restores what they
 * had before: an earlier decision (e.g. "let in"), else the rules. */
export async function holdSender(m: MessageSummary, learned: string) {
  const before = m.reason.kind === 'decision' ? m.reason.decision : null;
  if ((await act(null, () => api.screenerDecide(m.from_addr, 'digest'))) === undefined) return;
  app.open = null;
  app.learned = {
    text: learned,
    undo: () =>
      act<unknown>(null, () => (before ? api.screenerDecide(m.from_addr, before) : api.screenerForget(m.from_addr))).then(() => (app.learned = null)),
  };
}

const go_ = (v: View) => go(v);

export function go(view: View): void {
  app.view = view;
  app.open = null;
  app.digestPick = 0;
  if (view === 'focus') startFocus();
  // The account filter belongs to the Desk.
  if (view !== 'desk' && view !== 'focus') app.account = null;
}

export function visible(list: MessageSummary[]): MessageSummary[] {
  return app.account ? list.filter((m) => m.account === app.account) : list;
}

/** Account colour slot (0–3) by the account's position. */
/** What the reader calls an account ("Work"), else its name. */
export function accountLabel(name: string): string {
  return app.data?.accounts.find((a) => a.name === name)?.label ?? name;
}

export function accountSlot(name: string): number {
  const i = app.data?.accounts.findIndex((a) => a.name === name) ?? 0;
  return Math.max(0, i) % 4;
}

/** Digest mail split at the last delivery: [delivered, held for the next]. */
export function digestSplit(): [MessageSummary[], MessageSummary[]] {
  const last = lastSlot(new Date()).getTime() / 1000;
  const all = visible(app.data?.digest ?? []);
  return [all.filter((m) => m.ts === null || m.ts <= last), all.filter((m) => m.ts !== null && m.ts > last)];
}

/** Focus queue: people who need a reply first, then reading, then the rest. */
export function startFocus(): void {
  const rank = { reply: 0, read: 1, know: 2 } as const;
  const people = visible(app.data?.people ?? []);
  app.focus = { queue: [...people].sort((a, b) => rank[a.intent] - rank[b.intent]).map((m) => m.id), at: 0, replying: false };
}
