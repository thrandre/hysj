// Words on screen: wait labels, names, headlines, digest times. Pure, tested
// (words.test.ts).

import type { Reason } from './types.ts';

/** "12 min", "5 h", "1 day", "2 days" (Desk mockup). */
export function age(ts: number, now: number): string {
  const mins = Math.floor(Math.max(0, now - ts) / 60);
  if (mins < 60) return `${Math.max(1, mins)} min`;
  if (mins < 1440) return `${Math.floor(mins / 60)} h`;
  if (mins < 2880) return '1 day';
  return `${Math.floor(mins / 1440)} days`;
}

export function firstName(name: string): string {
  return name.trim().split(/\s+/)[0] ?? name;
}

export function initials(name: string): string {
  const letters = name
    .split(/\s+/)
    .map((w) => [...w].find((c) => /[\p{L}\p{N}]/u.test(c)))
    .filter((c): c is string => !!c)
    .slice(0, 2)
    .join('')
    .toUpperCase();
  return letters || '?';
}

/** "4 people are waiting on you." — at night they can wait (Desk — dark). */
export function peopleHeadline(waiting: number, hour: number): string {
  const night = hour < 6 || hour >= 21;
  if (waiting === 0) return night ? 'Nobody is waiting on you. Good night.' : 'Nobody is waiting on you. Enjoy the quiet.';
  if (waiting === 1) return night ? 'One person is still waiting — it can wait till morning.' : 'One person is waiting on you.';
  return night ? `${waiting} people are still waiting — they can wait till morning.` : `${waiting} people are waiting on you.`;
}

/** A subject as shown: "(no subject)" when empty. Emoji are kept: the webview draws them. */
export function subject(s: string): string {
  return s.trim() ? s : '(no subject)';
}

export function reasonText(r: Reason): string {
  switch (r.kind) {
    case 'decision':
      return r.decision === 'allow' ? 'You let them in' : r.decision === 'digest' ? 'You sent them to the Digest' : 'You blocked them';
    case 'bulk':
      return `Bulk mail (${r.signal})`;
    case 'no_reply':
      return 'Sent from a no-reply address';
    case 'emailed_before':
      return r.times === 1 ? "You've emailed them before" : `You've emailed them ${r.times} times`;
    case 'introduced':
      return `Introduced by ${r.by}`;
    case 'one_time_code':
      return 'One-time code or reset link';
    case 'delivery_service':
      return `Sent through ${r.service}`;
    case 'receipt':
      return 'A receipt or order confirmation';
    case 'first_time':
      return 'First-time sender';
  }
}

/** Digest delivery hours, local time (as hysj-core's digest.rs): the
 * reader's, set from each snapshot; 08:00 and 16:00 until they choose. */
let SLOTS = [8, 16];

export function setDigestHours(hours: number[]) {
  if (hours.length) SLOTS = [...hours].sort((a, b) => a - b);
}

export function lastSlot(now: Date): Date {
  for (let back = 0; back < 2; back++) {
    for (const h of [...SLOTS].reverse()) {
      const at = new Date(now.getFullYear(), now.getMonth(), now.getDate() - back, h, 0, 0, 0);
      if (at <= now) return at;
    }
  }
  throw new Error('a slot within the last day');
}

export function nextSlot(now: Date): Date {
  const last = lastSlot(now);
  const later = SLOTS.map((h) => new Date(last.getFullYear(), last.getMonth(), last.getDate(), h)).find((d) => d > last);
  return later ?? new Date(last.getFullYear(), last.getMonth(), last.getDate() + 1, SLOTS[0]);
}

export function hhmm(d: Date): string {
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
}

export const weekday = (d: Date) => d.toLocaleDateString('en-GB', { weekday: 'long' });

/** "---------- Forwarded message ---------", "Begin forwarded message:", translations. */
export function isForwardHeader(text: string): boolean {
  const t = text.trim().replace(/^[-\s]+/, '').toLowerCase();
  return ['forwarded message', 'videresendt melding', 'vidarebefordrat meddelande', 'videresendt meddelelse', 'begin forwarded message'].some(
    (p) => t.startsWith(p),
  );
}

/** "On … wrote:" / "… skrev …:" — the line that introduces a quote. */
export function isAttribution(text: string): boolean {
  const t = text.trim().toLowerCase();
  return t.length < 300 && t.endsWith(':') && (t.includes('wrote') || t.includes('skrev') || t.includes('a écrit'));
}

/** The host a link really goes to ("" if it isn't a web link). */
export function linkHost(href: string): string {
  try {
    const u = new URL(href);
    return u.protocol === 'http:' || u.protocol === 'https:' ? u.hostname.replace(/^www\./, '') : '';
  } catch {
    return '';
  }
}

/** When a link's text names one site but it goes to another: the warning to show. */
export function linkWarning(text: string, href: string): string | null {
  const host = linkHost(href);
  const shown = text.trim().toLowerCase().match(/^(?:https?:\/\/)?(?:www\.)?([a-z0-9-]+(?:\.[a-z0-9-]+)+)(?:[/?#].*)?$/);
  if (!host || !shown) return null;
  const named = shown[1];
  const same = host === named || host.endsWith('.' + named) || named.endsWith('.' + host);
  return same ? null : `Says ${named}, but goes to ${host}`;
}

/** Hint under a reply's From picker, and whether it's a warning (as the egui app). */
export function replyHint(
  from: string,
  arrivedOn: string,
  identities: { account: string; address: string }[],
  sender: string,
): [string, boolean] {
  if (from.toLowerCase() === arrivedOn.toLowerCase()) return ['where it arrived', false];
  const account = (a: string) => identities.find((i) => i.address.toLowerCase() === a.toLowerCase())?.account;
  if (account(from) && account(from) === account(arrivedOn)) return ['alias, same account', false];
  return [`${firstName(sender)} wrote to ${arrivedOn}`, true];
}

export function fileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
