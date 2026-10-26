// The Archive: everything that left the Desk, and why. Built from what the
// app already holds: cleared mail, digest mail already delivered, blocked
// senders' mail and expired codes.

import type { BookingItem, CodeItem, MessageSummary, OrderItem } from './types.ts';
import { hhmm, nextSlot, reasonText, subject, when } from './words.ts';

export type ArchiveKind = 'done' | 'sorted' | 'blocked' | 'expired';

export type ArchiveItem = {
  id: string;
  kind: ArchiveKind;
  from: string;
  subject: string;
  why: string;
  ts: number;
  /** The message, for corrections (none for an expired code). */
  m: MessageSummary | null;
};

export const KINDS: Record<ArchiveKind, string> = {
  done: 'Done by you',
  sorted: 'Sorted by Hysj',
  blocked: 'Blocked',
  expired: 'Expired',
};

export type Sources = {
  done: MessageSummary[];
  /** Digest mail an edition has already delivered. */
  delivered: MessageSummary[];
  blocked: MessageSummary[];
  codes: CodeItem[];
  bookings: BookingItem[];
  orders: OrderItem[];
  retentionHours: number;
};

/** Newest first. */
export function archive(s: Sources, now: Date): ArchiveItem[] {
  const nowS = now.getTime() / 1000;
  const booked = new Set(s.bookings.map((b) => b.message));
  const ordered = new Set(s.orders.map((o) => o.receipt));
  const item = (m: MessageSummary, kind: ArchiveKind, why: string): ArchiveItem => ({
    id: m.id,
    kind,
    from: m.from,
    subject: subject(m.subject),
    why,
    ts: m.ts ?? 0,
    m,
  });
  const out: ArchiveItem[] = [
    ...s.done.map((m) =>
      item(
        m,
        'done',
        m.replied
          ? 'You replied, then marked it done.'
          : m.bucket === 'people'
            ? 'You marked it done on your Desk.'
            : m.bucket === 'digest'
              ? 'You marked it read in the Digest.'
              : 'You marked it done.',
      ),
    ),
    ...s.delivered.map((m) => {
      const shown = m.ts === null ? 'Shown in an earlier digest.' : `Shown in the digest at ${when(nextSlot(new Date(m.ts * 1000)), now)}.`;
      const filed = booked.has(m.id) ? ' Filed in Bookings.' : ordered.has(m.id) ? ' Filed in Orders.' : '';
      return item(m, 'sorted', `${reasonText(m.reason)}. ${shown}${filed}`);
    }),
    ...s.blocked.map((m) =>
      item(m, 'blocked', m.reason.kind === 'decision' ? 'You blocked this sender in the Screener.' : `${reasonText(m.reason)}.`),
    ),
    ...s.codes
      .filter((c) => c.expires <= nowS)
      .map((c) => {
        const left = Math.max(1, Math.ceil((c.expires + s.retentionHours * 3600 - nowS) / 3600));
        return {
          id: c.id,
          kind: 'expired' as const,
          from: c.service || c.sender_domain,
          subject: c.kind,
          why: `${c.reset ? 'Reset link' : 'One-time code'}, expired at ${when(new Date(c.expires * 1000), now)}. Deleted for good in ${left === 1 ? 'an hour' : `${left} hours`}.`,
          ts: c.arrived,
          m: null,
        };
      }),
  ];
  return out.sort((a, b) => b.ts - a.ts);
}

/** "Today", "Yesterday", "Monday", "1 October". */
export function day(ts: number, now: Date): string {
  const w = when(new Date(ts * 1000), now).replace(/^\d\d:\d\d (on )?/, '');
  return w[0].toUpperCase() + w.slice(1);
}

export const time = (ts: number) => hhmm(new Date(ts * 1000));

/** "This week: 412 emails archived. 9 you finished yourself, …" */
export function weekLine(items: ArchiveItem[], now: Date): string {
  const week = items.filter((i) => i.ts >= now.getTime() / 1000 - 7 * 86_400);
  const n = (k: ArchiveKind) => week.filter((i) => i.kind === k).length;
  if (!week.length) return 'Nothing left your Desk this week.';
  const parts = [
    n('done') && `${n('done')} you finished yourself`,
    n('sorted') && `${n('sorted')} Hysj sorted away`,
    n('blocked') && `${n('blocked')} from blocked senders`,
    n('expired') && `${n('expired')} expired ${n('expired') === 1 ? 'code' : 'codes'}`,
  ].filter(Boolean);
  return `This week: ${week.length === 1 ? 'one email' : `${week.length} emails`} archived. ${parts.join(', ')}.`;
}
