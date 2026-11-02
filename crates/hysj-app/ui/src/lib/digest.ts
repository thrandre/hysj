// The Digest as a short read: deliveries, the meta line, the summary sentence.
// Ported from the egui app (digest.test.ts).

import type { MessageSummary } from './types.ts';

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
