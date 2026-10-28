import { test } from 'node:test';
import assert from 'node:assert/strict';
import { dueLine, nextFriday, nudge } from './waiting.ts';
import type { WaitItem } from './types.ts';

test('waiting words', () => {
  const now = new Date(2026, 9, 28, 14, 0); // a Wednesday
  const s = (d: number, h: number) => new Date(2026, 9, d, h).getTime() / 1000;
  const w: WaitItem = { id: '1', name: 'Ingrid Solheim', address: 'i@example.com', from: 'me@example.com', sent: s(26, 10), subject: 'Re: Budget', asked: 'Could you send the draft by Friday?', due: s(30, 17), words: 'by Friday' };
  assert.deepEqual(dueLine(w, now), { text: 'Due Friday — you wrote “by Friday”', late: false });
  assert.deepEqual(dueLine({ ...w, due: s(27, 17), words: null }, now), { text: 'Overdue · past the usual 3 working days', late: true });
  assert.equal(dueLine({ ...w, words: null }, now).text, 'No rush · 3 days left on the default 3 working days');
  assert.equal(nudge(w, now), 'Hi Ingrid — just checking in on my email from Monday (“Budget”). Any news?\n\nThanks!');
  assert.match(nudge({ ...w, asked: 'Kan du sende utkastet?' }, now), /^Hei Ingrid/);
  assert.equal(nextFriday(now).getDate(), 30);
  assert.equal(nextFriday(new Date(2026, 9, 30, 8)).getDate(), 6);
});
