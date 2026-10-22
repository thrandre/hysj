import { test } from 'node:test';
import assert from 'node:assert/strict';
import { bookingDetail, etaText, groupTrips, isTrip, tripTitle } from './findit.ts';
import type { BookingItem, OrderItem } from './types.ts';

const b = (kind: string, start: string, end: string | null, city: string | null): BookingItem => ({
  key: `${kind}:${start}`,
  kind,
  reference: 'R',
  title: 'T',
  start,
  end,
  start_ts: Date.parse(start) / 1000,
  provider: 'Air',
  city,
  seat: '12A',
  party: 4,
  moved_from: null,
  sources: 1,
  message: null,
});

test('trips and words', () => {
  const out = b('flight', '2026-11-06T07:30:00', '2026-11-06T08:25:00', 'Bergen');
  const hotel = b('hotel', '2026-11-06T15:00:00+01:00', '2026-11-08T11:00:00+01:00', 'Bergen');
  const home = b('flight', '2026-11-08T18:40:00', null, 'Oslo');
  const dinner = b('table', '2026-11-14T19:00:00', null, 'Oslo');
  const groups = groupTrips([out, hotel, home, dinner]);
  assert.deepEqual(groups.map((g) => g.length), [3, 1]);
  assert.ok(isTrip(groups[0]) && !isTrip(groups[1]));
  assert.equal(tripTitle(groups[0]), 'Bergen · 6–8 November');
  assert.equal(bookingDetail(out), '07:30 → 08:25 · Air · seat 12A');
  assert.equal(bookingDetail(hotel), 'Check-in from 15:00 · 2 nights');
  assert.equal(bookingDetail(dinner), '19:00 · table for 4');
});

test('order arrival', () => {
  const today = new Date(2026, 9, 20); // a Tuesday
  const o = (status: number, eta: string | null): OrderItem => ({
    key: 'k', store: 'S', number: '1', ordered: null, ordered_ts: null, items: [], total: null,
    status, eta, delivered_ts: null, tracking_url: null, return_by: null, sources: 1, receipt: null,
  });
  assert.equal(etaText(o(2, '2026-10-22'), today), 'Arrives Thursday');
  assert.equal(etaText(o(2, '2026-11-22'), today), 'Arrives 22 Nov');
  assert.equal(etaText(o(3, null), today), 'Out for delivery today');
  assert.equal(etaText(o(1, null), today), 'Ordered');
});
