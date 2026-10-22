import { test } from 'node:test';
import assert from 'node:assert/strict';
import { earlier, slots, summary, whenLine, window } from './digest.ts';

test('deliveries and words', () => {
  const now = new Date(2026, 9, 20, 14, 5); // Tuesday
  assert.deepEqual(window(0, now), [new Date(2026, 9, 19, 16).getTime() / 1000, new Date(2026, 9, 20, 8).getTime() / 1000]);
  assert.equal(window(3, now)[0], null);
  assert.deepEqual(earlier(now)[0], [1, 'Yesterday 16:00']);
  const s = slots(now);
  assert.equal(whenLine(s[0], s[1], 41), 'Tuesday · 08:00 digest · since 16:00 · 5 min');
  assert.equal(
    summary(3, 29, 4, 6),
    '3 newsletters and 29 notifications from 4 senders. Receipts and bookings are filed to Orders and Bookings (6).',
  );
  assert.equal(summary(0, 1, 1, 0), '1 notification from 1 sender.');
  assert.equal(summary(0, 0, 0, 0), '');
});
