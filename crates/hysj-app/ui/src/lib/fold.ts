// Folding quoted replies and signatures in a rendered mail (the frame's DOM,
// reached from the app's side: the frame itself runs no scripts). Forwards are
// content, never folded. If folding would hide everything, nothing is folded.

import { isAttribution, isForwardHeader } from './words.ts';

const QUOTES = [
  'blockquote',
  '.gmail_quote',
  '.yahoo_quoted',
  '.moz-cite-prefix',
  '.gmail_signature',
  '[data-smartmail="gmail_signature"]',
  '.moz-signature',
  '#Signature',
  '.hysj-quoted',
].join(',');

const textOf = (e: Element | null) => (e?.textContent ?? '').trim();

/** The text just before `e` in reading order (its previous element sibling, or its parent's). */
function before(e: Element): Element | null {
  let at: Element | null = e;
  while (at && !at.previousElementSibling) at = at.parentElement;
  return at?.previousElementSibling ?? null;
}

/** Marks quotes and signatures with `hysj-folded`; returns how many. */
export function fold(doc: Document): number {
  const marked: Element[] = [];
  const mark = (e: Element) => {
    e.classList.add('hysj-folded');
    marked.push(e);
  };
  for (const e of Array.from(doc.querySelectorAll(QUOTES))) {
    if (e.closest('.hysj-folded')) continue;
    const prev = before(e);
    // Gmail wraps forwards in gmail_quote; Apple Mail puts them in a cite
    // blockquote after "Begin forwarded message:".
    if (isForwardHeader(textOf(e)) || isForwardHeader(textOf(prev))) continue;
    mark(e);
    // "On … wrote:" goes with its quote.
    if (prev && isAttribution(textOf(prev)) && !prev.closest('.hysj-folded')) mark(prev);
  }
  // Outlook: everything from the reply header on is the earlier thread (and the rule above it).
  const outlook = doc.querySelector('#divRplyFwdMsg, [id^="appendonsend"]');
  if (outlook && !outlook.closest('.hysj-folded') && !isForwardHeader(textOf(outlook))) {
    let at: Element | null = outlook;
    const prev = outlook.previousElementSibling;
    if (prev?.tagName === 'HR') mark(prev);
    while (at) {
      mark(at);
      at = at.nextElementSibling;
    }
  }
  // Show the raw thing rather than nothing.
  const visible = Array.from(doc.body.querySelectorAll('*'))
    .filter((e) => !e.closest('.hysj-folded'))
    .some((e) => e.children.length === 0 && textOf(e).length > 0);
  const bodyText = Array.from(doc.body.childNodes).some((n) => n.nodeType === 3 && (n.textContent ?? '').trim());
  if (!visible && !bodyText) {
    for (const e of marked) e.classList.remove('hysj-folded');
    return 0;
  }
  return marked.length;
}
