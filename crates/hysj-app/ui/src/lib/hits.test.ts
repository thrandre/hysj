import { test } from 'node:test';
import assert from 'node:assert/strict';
import { isQuestion } from './hits.ts';

test('questions go to Ask your mail, commands stay commands', () => {
  assert.ok(isQuestion('What’s the wifi password at the cabin?'));
  assert.ok(isQuestion('når betalte jeg rørleggeren'));
  assert.ok(!isQuestion('reply to Aisha'));
  assert.ok(!isQuestion('what'));
  assert.ok(!isQuestion('latest login code'));
});
