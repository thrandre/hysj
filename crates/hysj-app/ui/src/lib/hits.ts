// What the command bar can find and do (Desk mockup: "Ask or do anything").
// ponytail: word matching over what the app has loaded; semantic search over
// all mail comes with the embeddings tier.

import type { View } from './app.svelte.ts';
import type { Snapshot } from './types.ts';
import { age, firstName, nextSlot, subject, when } from './words.ts';

export type Go =
  | { open: string; view: View }
  | { view: View }
  | { copy: string; key: string }
  | { compose: string }
  | { sync: true }
  | { search: string }
  | { account: string | null };

export interface Hit {
  kind: string;
  label: string;
  hint: string;
  go: Go;
}

/** Every word of the query appears in the text. */
export function matches(query: string, text: string): boolean {
  const hay = text.toLowerCase();
  return query
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((w) => hay.includes(w));
}

export function grouped(code: string): string {
  const split = { 6: 3, 7: 4, 8: 4 }[code.length];
  return split ? `${code.slice(0, split)} ${code.slice(split)}` : code;
}

export function hits(s: Snapshot, now: number): Hit[] {
  const out: Hit[] = [];
  for (const m of s.people) {
    const reply = m.intent === 'reply';
    out.push({
      kind: reply ? 'Reply' : 'Person',
      label: reply ? `Reply to ${m.from}` : m.from,
      hint: `${m.gist ?? subject(m.subject)}${m.ts ? ` · ${age(m.ts, now)}` : ''}`,
      go: { open: m.id, view: 'desk' },
    });
  }
  for (const c of s.codes.filter((c) => c.expires > now && !c.reset)) {
    out.push({
      kind: 'Code',
      label: `Copy ${c.kind} ${grouped(c.code ?? '')}`,
      hint: `${c.service} · expires in ${Math.ceil((c.expires - now) / 60)} min · login code`,
      go: { copy: c.code ?? '', key: `code:${c.id}` },
    });
  }
  for (const b of s.bookings.filter((b) => b.start_ts === null || b.start_ts >= now - 12 * 3600)) {
    out.push({ kind: 'Booking', label: b.title, hint: [b.reference, b.provider, b.city].filter(Boolean).join(' · '), go: { view: 'bookings' } });
  }
  for (const o of s.orders) {
    out.push({
      kind: 'Order',
      label: o.items.length ? o.items.join(', ') : `Order ${o.number} from ${o.store}`,
      hint: o.store,
      go: { view: 'orders' },
    });
  }
  out.push({ kind: 'Digest', label: 'Open the digest', hint: `next ${when(nextSlot(new Date(now * 1000)), new Date(now * 1000))}`, go: { view: 'digest' } });
  const senders = [...new Map(s.screener.map((m) => [m.from_addr.toLowerCase(), m.from])).values()];
  if (senders.length)
    out.push({ kind: 'Screener', label: `Review ${senders.length} new senders`, hint: senders.slice(0, 3).join(', '), go: { view: 'screener' } });
  out.push({ kind: 'Compose', label: 'Write a new email', hint: '⌘N', go: { compose: '' } });
  for (const k of s.contacts.slice(0, 200))
    out.push({ kind: 'Compose', label: `Write to ${k.name ?? k.address}`, hint: k.address, go: { compose: k.address } });
  if (s.accounts.length > 1) {
    for (const a of s.accounts) out.push({ kind: 'Filter', label: `Show ${a.label ?? a.name} only`, hint: a.address, go: { account: a.name } });
    out.push({ kind: 'Filter', label: 'Show all accounts', hint: '', go: { account: null } });
  }
  out.push(
    { kind: 'Sync', label: 'Check for new mail', hint: 'all accounts', go: { sync: true } },
    { kind: 'Focus', label: 'One at a time', hint: 'R reply · E done · H later · → skip', go: { view: 'focus' } },
    { kind: 'Sent', label: 'Sent mail', hint: 'newest first', go: { view: 'sent' } },
    { kind: 'Done', label: 'Done — cleared mail', hint: 'restore from here', go: { view: 'done' } },
    { kind: 'Settings', label: 'Add an account', hint: 'Outlook.com, iCloud or your own domain', go: { view: 'settings' } },
    { kind: 'Settings', label: 'Appearance', hint: 'light, dark or follow the system', go: { view: 'settings' } },
  );
  for (const m of s.sent.slice(0, 300)) out.push({ kind: 'Sent', label: subject(m.subject), hint: `To ${m.to ?? ''}`, go: { open: m.id, view: 'sent' } });
  for (const m of s.done.slice(0, 300)) out.push({ kind: 'Done', label: subject(m.subject), hint: m.from, go: { open: m.id, view: 'done' } });
  return out;
}

/** Chips under the empty field, only for things that exist: [label, query]. */
export function suggestions(s: Snapshot, now: number): [string, string][] {
  const out: [string, string][] = [];
  const reply = s.people.find((m) => m.intent === 'reply');
  if (reply) out.push([`Reply to ${firstName(reply.from)}`, `reply ${firstName(reply.from)}`]);
  if (s.codes.some((c) => c.expires > now && !c.reset)) out.push(['Latest login code', 'code']);
  const b = s.bookings.find((b) => b.start_ts !== null && b.start_ts >= now);
  if (b) {
    const what = b.city ?? b.title;
    out.push([`Where’s my ${what} booking?`, what]);
  }
  const o = s.orders.find((o) => o.status < 4 && o.items.length);
  if (o) out.push([`Where is my ${o.items[0]}?`, o.items[0]]);
  return out;
}
