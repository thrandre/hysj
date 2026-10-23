import { test } from 'node:test';
import assert from 'node:assert/strict';
import { earlier, summary, window } from './digest.ts';
import { nextSlot, setDigestHours, when } from './words.ts';

test('deliveries and words', () => {
  const now = new Date(2026, 9, 20, 14, 5); // Tuesday
  assert.deepEqual(window(0, now), [new Date(2026, 9, 19, 16).getTime() / 1000, new Date(2026, 9, 20, 8).getTime() / 1000]);
  assert.equal(window(3, now)[0], null);
  assert.deepEqual(earlier(now)[0], [1, '16:00 yesterday']);
  assert.equal(when(new Date(2026, 9, 20, 12), now), '12:00 today');
  assert.equal(when(new Date(2026, 9, 21, 12), now), '12:00 tomorrow');
  assert.equal(when(new Date(2026, 9, 19, 19), now), '19:00 yesterday');
  assert.equal(when(new Date(2026, 9, 17, 8), now), '08:00 on Saturday');
  assert.equal(when(new Date(2026, 9, 1, 8), now), '08:00 on 1 October');
  // New hours apply at once: no 16:00 left over.
  setDigestHours([12]);
  assert.equal(when(nextSlot(now), now), '12:00 tomorrow');
  assert.deepEqual(earlier(now)[0], [1, '12:00 yesterday']);
  setDigestHours([8, 16]);
  assert.equal(
    summary(3, 29, 4, 6),
    '3 newsletters and 29 notifications from 4 senders. Receipts and bookings are filed to Orders and Bookings (6).',
  );
  assert.equal(summary(0, 1, 1, 0), '1 notification from 1 sender.');
  assert.equal(summary(0, 0, 0, 0), '');
});
