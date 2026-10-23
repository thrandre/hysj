// The Digest as a short read: deliveries, the meta line, the summary sentence.
// Ported from the egui app (digest.test.ts).

import type { MessageSummary } from './types.ts';
import { lastSlot, when } from './words.ts';

/** The latest delivery and the three before it, newest first. */
export function slots(now: Date): Date[] {
  const s0 = lastSlot(now);
  const s1 = lastSlot(new Date(s0.getTime() - 1000));
  const s2 = lastSlot(new Date(s1.getTime() - 1000));
  const s3 = lastSlot(new Date(s2.getTime() - 1000));
  return [s0, s1, s2, s3];
}

/** Which messages a pick shows: (after, up to and including), unix seconds. 0 latest, 1–2 earlier, 3 older. */
export function window(pick: number, now: Date): [number | null, number] {
  const s = slots(now).map((d) => d.getTime() / 1000);
  if (pick === 0) return [s[1], s[0]];
  if (pick === 1) return [s[2], s[1]];
  if (pick === 2) return [s[3], s[2]];
  return [null, s[3]];
}

/** Earlier deliveries for the top bar: [pick, label]. */
export function earlier(now: Date): [number, string][] {
  const s = slots(now);
  return [
    [1, when(s[1], now)],
    [2, when(s[2], now)],
    [3, 'Older'],
  ];
}

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
export { plural };

/** The paragraph under the headline, from counts. */
export function summary(newsletters: number, notifications: number, senders: number, filed: number): string {
  const parts: string[] = [];
  if (newsletters) parts.push(plural(newsletters, 'newsletter', 'newsletters'));
  if (notifications) parts.push(`${plural(notifications, 'notification', 'notifications')} from ${plural(senders, 'sender', 'senders')}`);
  let s = parts.length === 0 ? '' : parts.length === 1 ? `${parts[0]}.` : `${parts[0]} and ${parts[1]}.`;
  if (filed) s += ` Receipts and bookings are filed to Orders and Bookings (${filed}).`;
  s = s.trim();
  return s ? s[0].toUpperCase() + s.slice(1) : '';
}

export const isNewsletter = (m: MessageSummary) => m.reason.kind === 'bulk' && m.reason.signal !== 'Auto-Submitted';

/** Grouped by sender, biggest group first (lists arrive newest first). */
export function bySender(items: MessageSummary[]): MessageSummary[][] {
  const groups: MessageSummary[][] = [];
  for (const m of items) {
    const g = groups.find((g) => g[0].from_addr.toLowerCase() === m.from_addr.toLowerCase());
    if (g) g.push(m);
    else groups.push([m]);
  }
  return groups.sort((a, b) => b.length - a.length);
}
