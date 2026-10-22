import { test } from 'node:test';
import assert from 'node:assert/strict';
import { age, initials, lastSlot, nextSlot, peopleHeadline } from './words.ts';

test('wait labels', () => {
  const now = 1_000_000;
  assert.equal(age(now - 30, now), '1 min');
  assert.equal(age(now - 5 * 3600, now), '5 h');
  assert.equal(age(now - 26 * 3600, now), '1 day');
  assert.equal(age(now - 2 * 86400, now), '2 days');
});

test('initials', () => {
  assert.equal(initials('Aisha Rahman'), 'AR');
  assert.equal(initials('pappa'), 'P');
  assert.equal(initials('"Øystein" Berg'), 'ØB');
});

test('headlines', () => {
  assert.equal(peopleHeadline(4, 14), '4 people are waiting on you.');
  assert.equal(peopleHeadline(0, 23), 'Nobody is waiting on you. Good night.');
});

test('digest slots', () => {
  const at = (h: number, m = 0) => new Date(2026, 9, 21, h, m);
  assert.deepEqual(lastSlot(at(14, 5)), at(8));
  assert.deepEqual(nextSlot(at(14, 5)), at(16));
  assert.deepEqual(lastSlot(at(7, 59)), new Date(2026, 9, 20, 16));
  assert.deepEqual(nextSlot(at(17)), new Date(2026, 9, 22, 8));
});

import { isAttribution, isForwardHeader, linkWarning, replyHint, fileSize } from './words.ts';

test('forwards and attributions', () => {
  assert.ok(isForwardHeader('---------- Forwarded message ---------'));
  assert.ok(isForwardHeader('---------- Videresendt melding ----------'));
  assert.ok(!isForwardHeader('-----Original Message-----'));
  assert.ok(isAttribution('On Mon, 19 Oct 2026, Aisha Rahman <a@x> wrote:'));
  assert.ok(isAttribution('man. 19. okt. 2026 kl. 09:00 skrev Ingrid:'));
  assert.ok(!isAttribution('Here is what I wrote.'));
});

test('links that lie', () => {
  assert.equal(linkWarning('www.dnb.no', 'https://dnb-secure.example.ru/login'), 'Says dnb.no, but goes to dnb-secure.example.ru');
  assert.equal(linkWarning('dnb.no', 'https://www.dnb.no/login'), null);
  assert.equal(linkWarning('Log in', 'https://evil.example'), null, 'only when the text names a site');
  assert.equal(linkWarning('mail.google.com', 'https://google.com/x'), null);
});

test('reply hints and sizes', () => {
  const ids = [
    { account: 'gmail', address: 'me@gmail.com' },
    { account: 'gmail', address: 'alias@me.example' },
    { account: 'work', address: 'me@work.example' },
  ];
  assert.deepEqual(replyHint('ME@gmail.com', 'me@gmail.com', ids, 'Aisha R'), ['where it arrived', false]);
  assert.deepEqual(replyHint('alias@me.example', 'me@gmail.com', ids, 'Aisha R'), ['alias, same account', false]);
  assert.deepEqual(replyHint('me@work.example', 'me@gmail.com', ids, 'Aisha Rahman'), ['Aisha wrote to me@gmail.com', true]);
  assert.equal(fileSize(2_500_000), '2.4 MB');
});

import { matches, grouped } from './hits.ts';

test('command bar matching', () => {
  assert.ok(matches('bergen book', 'Booking Oslo → Bergen K7P2QX'));
  assert.ok(matches('REPLY ingrid', 'Reply Reply to Ingrid Solheim'));
  assert.ok(!matches('dock', 'Order Oak boards'));
  assert.equal(grouped('482913'), '482 913');
});
