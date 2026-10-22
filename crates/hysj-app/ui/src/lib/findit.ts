// "Find it fast": bookings, orders and codes in words. Ported from the egui
// app's tested helpers (findit.test.ts).

import type { BookingItem, OrderItem } from './types.ts';

/** Date and "HH:MM" from an ISO string as written (local to the place). */
export function localParts(iso: string | null): { date: Date; time: string | null } | null {
  if (!iso || iso.length < 10) return null;
  const [y, m, d] = iso.slice(0, 10).split('-').map(Number);
  if (!y || !m || !d) return null;
  const time = iso[13] === ':' ? iso.slice(11, 16) : null;
  return { date: new Date(y, m - 1, d), time };
}

const MONTHS = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'];
const dayMonth = (d: Date) => `${d.getDate()} ${MONTHS[d.getMonth()]}`;
const shortMonth = (d: Date) => MONTHS[d.getMonth()].slice(0, 3);
export { shortMonth };

const isoTs = (iso: string | null) => (iso ? Date.parse(iso.length === 19 ? iso : iso) / 1000 : NaN);

/** Bookings that start within two days of the previous ones ending form a group. Input sorted by start. */
export function groupTrips(items: BookingItem[]): BookingItem[][] {
  const groups: BookingItem[][] = [];
  const ends = (b: BookingItem) => {
    const e = isoTs(b.end);
    return Number.isFinite(e) ? e : b.start_ts;
  };
  for (const b of items) {
    const last = groups.at(-1);
    const end = last ? Math.max(...last.map(ends).filter((x): x is number => x !== null)) : null;
    if (last && end !== null && b.start_ts !== null && b.start_ts - end <= 2 * 86400) last.push(b);
    else groups.push([b]);
  }
  return groups;
}

export const isTrip = (g: BookingItem[]) => g.length >= 2 && g.some((b) => ['flight', 'train', 'bus', 'hotel'].includes(b.kind));

/** "Bergen · 6–8 November". */
export function tripTitle(g: BookingItem[]): string {
  const city = (g.find((b) => b.kind === 'hotel') ?? g[0])?.city ?? 'Trip';
  const first = localParts(g[0]?.start ?? null)?.date;
  const days = g.map((b) => localParts(b.end ?? b.start)?.date).filter((d): d is Date => !!d);
  const last = days.length ? new Date(Math.max(...days.map((d) => d.getTime()))) : undefined;
  if (!first || !last) return city;
  if (first.getTime() === last.getTime()) return `${city} · ${dayMonth(first)}`;
  if (first.getMonth() === last.getMonth()) return `${city} · ${first.getDate()}–${dayMonth(last)}`;
  return `${city} · ${dayMonth(first)} – ${dayMonth(last)}`;
}

/** The grey line under a booking's title. */
export function bookingDetail(b: BookingItem): string {
  const time = (s: string | null) => localParts(s)?.time ?? null;
  const parts: string[] = [];
  if (['flight', 'train', 'bus'].includes(b.kind)) {
    const [a, c] = [time(b.start), time(b.end)];
    if (a && c) parts.push(`${a} → ${c}`);
    else if (a) parts.push(a);
    if (b.provider) parts.push(b.provider);
    if (b.seat) parts.push(`seat ${b.seat}`);
  } else if (b.kind === 'hotel') {
    const t = time(b.start);
    if (t) parts.push(`Check-in from ${t}`);
    const [a, c] = [localParts(b.start)?.date, localParts(b.end)?.date];
    if (a && c) {
      const n = Math.round((c.getTime() - a.getTime()) / 86400000);
      parts.push(n === 1 ? '1 night' : `${n} nights`);
    }
  } else if (b.kind === 'table') {
    const t = time(b.start);
    if (t) parts.push(t);
    if (b.party) parts.push(`table for ${b.party}`);
  } else {
    const t = time(b.start);
    if (t) parts.push(t);
    if (b.provider) parts.push(b.provider);
  }
  return parts.join(' · ');
}

const sameDay = (a: Date, b: Date) => a.toDateString() === b.toDateString();

/** "Arrives Thursday", "Out for delivery today", "Delivered 13 Oct". */
export function etaText(o: OrderItem, today: Date): string {
  if (o.status === 4) return o.delivered_ts ? `Delivered ${new Date(o.delivered_ts * 1000).getDate()} ${shortMonth(new Date(o.delivered_ts * 1000))}` : 'Delivered';
  if (o.status === 3) return 'Out for delivery today';
  const d = localParts(o.eta)?.date;
  if (d) {
    if (sameDay(d, today)) return 'Arrives today';
    const days = Math.round((d.getTime() - new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime()) / 86400000);
    if (days >= 1 && days < 7) return `Arrives ${d.toLocaleDateString('en-GB', { weekday: 'long' })}`;
    return `Arrives ${d.getDate()} ${shortMonth(d)}`;
  }
  return o.status === 2 ? 'Shipped' : 'Ordered';
}

export function codesHeadline(codes: number, resets: number): string {
  if (codes === 0) return resets === 0 ? 'No codes right now.' : resets === 1 ? 'A password reset is waiting.' : `${resets} password resets are waiting.`;
  return codes === 1 ? 'One code is live right now.' : `${codes} codes are live right now.`;
}

const when = (days: number) => (days <= 0 ? 'today' : days === 1 ? 'tomorrow' : `in ${days} days`);

/** "Bergen in 17 days. Dinner on 14 November." */
export function bookingsHeadline(groups: BookingItem[][], today: Date): string {
  const parts: string[] = [];
  for (const g of groups.slice(0, 2)) {
    const d = localParts(g[0]?.start ?? null)?.date;
    if (!d) continue;
    const days = Math.round((d.getTime() - new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime()) / 86400000);
    if (isTrip(g)) parts.push(`${tripTitle(g).split(' · ')[0]} ${when(days)}.`);
    else if (!parts.length) parts.push(`${g[0].title} ${when(days)}.`);
    else parts.push(`${g[0].title} on ${dayMonth(d)}.`);
  }
  return parts.length ? parts.join(' ') : 'No bookings coming up.';
}

const WORDS = ['No', 'One', 'Two', 'Three', 'Four', 'Five', 'Six', 'Seven', 'Eight', 'Nine'];
const count = (n: number) => WORDS[n] ?? String(n);

/** Orders headline per filter: 0 on the way, 1 delivered, 2 returns. */
export function ordersHeadline(tab: number, shown: OrderItem[], today: Date): string {
  const n = shown.length;
  if (tab === 0) {
    let s = n === 0 ? 'Nothing on the way.' : n === 1 ? 'One parcel on the way.' : `${count(n)} parcels on the way.`;
    const soon = shown.find((o) => etaText(o, today).endsWith('today') && o.items.length);
    if (soon) s += ` The ${soon.items[0]} arrives today.`;
    return s;
  }
  if (tab === 1) return n === 0 ? 'Nothing delivered yet.' : 'Everything delivered.';
  return n === 0 ? 'No return windows open.' : n === 1 ? 'One return window is still open.' : `${count(n)} return windows are still open.`;
}
