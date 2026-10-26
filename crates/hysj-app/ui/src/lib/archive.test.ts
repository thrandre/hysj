import { test } from 'node:test';
import assert from 'node:assert/strict';
import { archive, day, weekLine } from './archive.ts';
import type { MessageSummary } from './types.ts';

test('the archive says why', () => {
  const now = new Date(2026, 9, 20, 14, 5);
  const at = (d: number, h: number) => new Date(2026, 9, d, h).getTime() / 1000;
  const m = (id: string, bucket: MessageSummary['bucket'], ts: number, reason: MessageSummary['reason']): MessageSummary =>
    ({ id, account: 'a', from: id, from_addr: `${id}@example.com`, subject: '', date: null, ts, bucket, reason, intent: 'know', replied: false, arrived_on: '', replies: [], tags: [] }) as MessageSummary;
  const items = archive(
    {
      done: [m('kept', 'people', at(20, 9), { kind: 'emailed_before', times: 1 })],
      delivered: [m('shop', 'digest', at(19, 12), { kind: 'receipt' })],
      blocked: [m('spam', 'blocked', at(18, 3), { kind: 'decision', decision: 'block' })],
      codes: [{ id: 'c', kind: 'Sign-in code', reset: false, service: 'Bank', sender_domain: 'bank.example', code: '13', link: null, link_host: null, link_ok: false, arrived: at(20, 8), expires: at(20, 9), reported: false }],
      bookings: [],
      orders: [{ receipt: 'shop' } as never],
      retentionHours: 24,
    },
    now,
  );
  assert.deepEqual(
    items.map((i) => [i.kind, i.why]),
    [
      ['done', 'You marked it done on your Desk.'],
      ['expired', 'One-time code, expired at 09:00 today. Deleted for good in 19 hours.'],
      ['sorted', 'A receipt or order confirmation. Shown in the digest at 16:00 yesterday. Filed in Orders.'],
      ['blocked', 'You blocked this sender in the Screener.'],
    ],
  );
  assert.equal(items[1].subject, 'Sign-in code');
  assert.deepEqual(items.map((i) => day(i.ts, now)), ['Today', 'Today', 'Yesterday', 'Sunday']);
  assert.equal(weekLine(items, now), 'This week: 4 emails archived. 1 you finished yourself, 1 Hysj sorted away, 1 from blocked senders, 1 expired code.');
});
