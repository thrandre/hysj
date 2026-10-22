<script lang="ts">
  // One at a time (Focus mockup): Reply (R), Done (E), Later (H), Skip (→).
  // ponytail: Later moves the message to the end of this session's queue; a
  // real snooze (back on the Desk at a chosen time) needs a store column.
  import { act, app, go, startFocus } from '../lib/app.svelte.ts';
  import { api } from '../lib/api.ts';
  import { age, hhmm, initials, nextSlot, reasonText, subject } from '../lib/words.ts';
  import { accountSlot } from '../lib/app.svelte.ts';
  import ReplyBox from '../components/ReplyBox.svelte';
  import MailFrame from '../components/MailFrame.svelte';
  import type { MessageSummary } from '../lib/types.ts';

  const byId = (id: string | undefined) => app.data!.people.find((m) => m.id === id);
  // Skip anything cleared elsewhere meanwhile.
  let current = $derived.by(() => {
    for (let i = app.focus.at; i < app.focus.queue.length; i++) {
      const m = byId(app.focus.queue[i]);
      if (m) return { m, at: i };
    }
    return null;
  });
  let next = $derived(current ? app.focus.queue.slice(current.at + 1).map(byId).find(Boolean) : undefined);
  let view = $derived(current ? app.views[current.m.id] : undefined);
  const tags = { reply: 'needs a reply', read: 'worth reading', know: 'just so you know' } as const;

  $effect(() => {
    const id = current?.m.id;
    if (id && !app.views[id])
      api.messageView(id).then(
        (v) => {
          app.views[id] = v;
        },
        (e) => {
          app.error = String(e);
        },
      );
  });

  function step(s: 'reply' | 'done' | 'later' | 'skip') {
    if (!current) return;
    const { m, at } = current;
    app.focus.replying = s === 'reply';
    if (s === 'reply') return;
    if (s === 'done') void act(null, () => api.done([m.id]));
    if (s === 'later') app.focus.queue.push(m.id);
    app.focus.at = at + 1;
  }

  function keys(e: KeyboardEvent) {
    if ((e.target as HTMLElement | null)?.closest('input, textarea, select') || e.metaKey || e.ctrlKey) return;
    const k = { r: 'reply', e: 'done', h: 'later', ArrowRight: 'skip' }[e.key] as Parameters<typeof step>[0] | undefined;
    if (k) {
      e.preventDefault();
      step(k);
    }
  }
  const text = (m: MessageSummary) => m.gist ?? subject(m.subject);
</script>

<svelte:window onkeydown={keys} />

<div class="column narrow">
  {#if current}
    {@const m = current.m}
    <p class="meta">{current.at + 1} of {app.focus.queue.length} · {tags[m.intent]}</p>
    <article class="card">
      <div class="who">
        <span class="avatar" style:background="var(--acct-{accountSlot(m.account)}-bg)" aria-hidden="true">{initials(m.from)}</span>
        <div><strong>{m.from}</strong><span class="why">{reasonText(m.reason)}</span></div>
        <span class="mono when">{m.ts ? age(m.ts, Date.now() / 1000) : ''}</span>
      </div>
      <h1>{text(m)}</h1>
      {#if view}
        <!-- Mail only ever renders in the sandboxed frame, never in the app's page. -->
        <MailFrame {view} onmailto={(to) => (app.compose = { to })} />
      {/if}
      {#if app.focus.replying}
        {#key m.id}<ReplyBox {m} autofocus onsent={() => step('skip')} />{/key}
      {/if}
    </article>
    <div class="actions">
      <button class="btn primary big" onclick={() => step('reply')}>Reply <kbd>R</kbd></button>
      <button class="btn big plain" onclick={() => step('done')}>Done <kbd>E</kbd></button>
      <button class="btn big plain" onclick={() => step('later')}>Later <kbd>H</kbd></button>
      <button class="btn big ghost push" onclick={() => step('skip')}>Skip <kbd>→</kbd></button>
    </div>
    {#if next}<p class="next">Up next: <strong>{next.from}</strong> — {text(next)}</p>{/if}
  {:else}
    <div class="end">
      <h1>That’s everyone.</h1>
      <p>Nothing else will reach you until the {hhmm(nextSlot(new Date()))} digest.</p>
      <button class="btn raised" onclick={startFocus}>Start over</button>
      <button class="btn ghost" onclick={() => go('desk')}>Back to Desk</button>
    </div>
  {/if}
</div>

<style>
  .narrow {
    max-width: 680px;
    padding-top: 48px;
    display: flex;
    flex-direction: column;
    gap: 22px;
  }
  .meta {
    margin: 0;
  }
  article {
    display: flex;
    flex-direction: column;
    gap: 20px;
    padding: 32px;
    border-radius: 28px;
    box-shadow: var(--shadow-lg);
  }
  .who {
    display: flex;
    align-items: center;
    gap: 14px;
  }
  .who div {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .who strong {
    font-size: 17px;
  }
  .why,
  .when {
    font-size: 13px;
    color: var(--muted);
  }
  .avatar {
    width: 48px;
    height: 48px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 600;
  }
  h1 {
    margin: 0;
    font-size: 28px;
    line-height: 1.25;
    font-weight: 600;
    letter-spacing: -0.02em;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }
  .big {
    height: 52px;
    padding: 0 20px;
    border-radius: 14px;
  }
  .plain {
    background: var(--surface);
    box-shadow: 0 4px 12px rgba(17, 17, 17, 0.05);
  }
  .push {
    margin-left: auto;
    color: var(--secondary);
  }
  kbd {
    font-family: var(--mono);
    font-size: 12px;
    padding: 2px 7px;
    border-radius: 6px;
    background: var(--field);
    color: var(--secondary);
  }
  .primary kbd {
    background: rgba(255, 255, 255, 0.18);
    color: inherit;
  }
  .next {
    margin: 0;
    color: var(--muted);
  }
  .next strong {
    color: var(--ink);
  }
  .end {
    margin-top: 80px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 14px;
    text-align: center;
  }
  .end h1 {
    font-size: 40px;
  }
  .end p {
    margin: 0;
    font-size: 18px;
    color: var(--secondary);
  }
</style>
