// Waiting on others: the words around each item (the daemon finds them).

import type { WaitItem } from './types.ts';
import { firstName, subject, weekday, when } from './words.ts';

/** "You wrote yesterday", "You wrote on Thursday", "You wrote on 14 October". */
export function wrote(ts: number, now: Date): string {
  return `You wrote ${when(new Date(ts * 1000), now).replace(/^\d\d:\d\d /, '')}`;
}

/** The due line, and whether it's late. */
export function dueLine(w: WaitItem, now: Date): { text: string; late: boolean } {
  const n = now.getTime() / 1000;
  const said = w.words ? `you wrote “${w.words}”` : null;
  if (n > w.due) return { text: `Overdue · ${said ?? 'past the usual 3 working days'}`, late: true };
  if (said) return { text: `Due ${weekday(new Date(w.due * 1000))} — ${said}`, late: false };
  const days = Math.ceil((w.due - n) / 86_400);
  return { text: `No rush · ${days === 1 ? '1 day' : `${days} days`} left on the default 3 working days`, late: false };
}

const NORWEGIAN = /[æøå]|\b(du|jeg|kan|vil|hei|takk|og)\b/i;

/** A polite follow-up, in the language you asked in. Edited before sending. */
export function nudge(w: WaitItem, now: Date): string {
  const first = firstName(w.name);
  const about = subject(w.subject).replace(/^((re|sv|fwd?|vs|aw):\s*)+/i, '');
  const day = when(new Date(w.sent * 1000), now).replace(/^\d\d:\d\d (on )?/, '');
  return NORWEGIAN.test(w.asked)
    ? `Hei ${first} — ville bare høre hvordan det går med dette («${about}»). Har du fått sett på det?\n\nTakk!`
    : `Hi ${first} — just checking in on my email from ${day} (“${about}”). Any news?\n\nThanks!`;
}

/** Next Friday at 09:00 (a week on, if it's Friday already). */
export function nextFriday(now: Date): Date {
  const d = new Date(now);
  d.setHours(9, 0, 0, 0);
  d.setDate(d.getDate() + (((5 - d.getDay() + 7) % 7) || 7));
  return d;
}
