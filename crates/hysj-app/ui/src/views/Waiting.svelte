<script lang="ts">
  // Waiting on others: mail you sent that asked for something. A nudge is a
  // draft you edit; it waits a few seconds after Send so Undo really undoes.
  import { act, app, go, toggle } from '../lib/app.svelte.ts';
  import { api } from '../lib/api.ts';
  import { clock } from '../lib/clock.svelte.ts';
  import type { WaitItem, Waiting } from '../lib/types.ts';
  import { dueLine, nextFriday, nudge, wrote } from '../lib/waiting.ts';
  import { initials, subject } from '../lib/words.ts';
  import Inline from '../components/Inline.svelte';

  const GRACE = 10;

  let data = $state<Waiting | null>(null);
  /** Open drafts, by item. */
  let drafts = $state<Record<string, string>>({});
  /** What happened here, kept in place after the list reloads. */
  let acted = $state<Record<string, { item: WaitItem; result: string; undo?: () => void }>>({});

  function load() {
    api.waiting().then(
      (w) => (data = w),
      (e) => (app.error = String(e)),
    );
  }
  $effect(() => {
    void app.data;
    load();
  });

  let now = $derived(new Date(clock.now * 1000));
  let items = $derived.by(() => {
    const here = new Set((data?.items ?? []).map((i) => i.id));
    return [...(data?.items ?? []), ...Object.values(acted).map((a) => a.item).filter((i) => !here.has(i.id))].sort((a, b) => a.due - b.due);
  });
  let open = $derived(items.filter((i) => !acted[i.id]));
  let late = $derived(open.filter((i) => dueLine(i, now).late).length);
  let headline = $derived(
    !open.length
      ? 'Nobody owes you an answer.'
      : open.length === 1
        ? `You’re waiting on one person${late ? ' — and they’re late.' : '.'}`
        : `You’re waiting on ${open.length} people${late ? ` — ${late === 1 ? 'one is' : `${late} are`} late.` : '.'}`,
  );

  function send(w: WaitItem) {
    const body = drafts[w.id];
    delete drafts[w.id];
    const timer = setTimeout(async () => {
      const r = await act('Sending the nudge…', () => api.send({ from: w.from, to: [w.address], cc: [], subject: '', body, reply_to: w.id }));
      acted[w.id] = {
        item: w,
        result: r ? `Nudge sent from ${w.from}. Hysj keeps waiting — you’ll see the answer on your Desk.` : 'The nudge wasn’t sent.',
      };
    }, GRACE * 1000);
    acted[w.id] = {
      item: w,
      result: 'Sending in a few seconds.',
      undo: () => {
        clearTimeout(timer);
        delete acted[w.id];
        drafts[w.id] = body;
      },
    };
  }

  async function later(w: WaitItem) {
    const until = nextFriday(now);
    if ((await act(null, () => api.waitingSet(w.id, 'later', Math.floor(until.getTime() / 1000)))) === undefined) return;
    acted[w.id] = { item: w, result: 'Hysj will remind you on Friday at 09:00.', undo: () => undo(w) };
  }

  async function stop(w: WaitItem) {
    if ((await act(null, () => api.waitingSet(w.id, 'stop'))) === undefined) return;
    acted[w.id] = { item: w, result: 'Stopped waiting.', undo: () => undo(w) };
  }

  async function undo(w: WaitItem) {
    if ((await act(null, () => api.waitingSet(w.id, 'active'))) === undefined) return;
    delete acted[w.id];
  }
</script>

<div class="column">
  <header>
    <p class="meta">Waiting on others · from what you sent</p>
    <h1 class="h1">{data ? headline : 'Looking through what you sent…'}</h1>
    <p class="lede">
      Hysj notices when you ask for something and keeps an eye out for the answer. When someone goes quiet past the deadline, it offers a polite nudge —
      you decide whether to send it.
    </p>
  </header>

  {#if data}
    <div class="items">
      {#each items as w (w.id)}
        {@const due = dueLine(w, now)}
        {@const a = acted[w.id]}
        <div class="item card">
          <div class="line">
            <span class="av" aria-hidden="true">{initials(w.name)}</span>
            <div class="text">
              <span class="who"><strong>{w.name}</strong><span class="sub">{wrote(w.sent, now)}</span></span>
              <span class="asked">{w.asked}</span>
              <span class="due" class:late={due.late}>{due.text}</span>
            </div>
          </div>
          {#if a}
            <div class="indent result">
              <span>{a.result}</span>
              {#if a.undo}<button class="btn small" onclick={a.undo}>Undo</button>{/if}
            </div>
          {:else if drafts[w.id] !== undefined}
            <div class="draft">
              <label for="draft-{w.id}">Follow-up to {w.name} · from {w.from} · in the same thread</label>
              <textarea id="draft-{w.id}" rows="4" bind:value={drafts[w.id]}></textarea>
              <div class="actions">
                <button class="btn primary" disabled={!drafts[w.id].trim()} onclick={() => send(w)}>Send nudge</button>
                <button class="btn ghost" onclick={() => delete drafts[w.id]}>Not now</button>
              </div>
            </div>
          {:else}
            <div class="indent actions">
              <button class="btn" class:primary={due.late} onclick={() => (drafts[w.id] = nudge(w, now))}>Draft a nudge</button>
              <button class="btn" onclick={() => later(w)}>Remind me Friday</button>
              <button class="btn ghost" onclick={() => stop(w)}>Stop waiting</button>
              <button class="btn ghost" aria-expanded={app.open === w.id} onclick={() => toggle(w.id)}>{app.open === w.id ? 'Close' : 'What you sent'}</button>
            </div>
          {/if}
          {#if app.open === w.id}<div class="indent"><Inline id={w.id} /></div>{/if}
        </div>
      {/each}
    </div>

    {#if data.answered.length}
      <section aria-labelledby="h-answered">
        <h2 id="h-answered">Answered this week</h2>
        {#each data.answered as r (r.id)}
          <div class="answered">
            <span>{r.name} answered your email about “{subject(r.subject)}”</span>
            {#if r.bucket === 'people'}
              <button class="link" onclick={() => (go('desk'), toggle(r.id))}>On your Desk</button>
            {:else}
              <button class="link" aria-expanded={app.open === r.id} onclick={() => toggle(r.id)}>{app.open === r.id ? 'Close' : 'Open'}</button>
            {/if}
          </div>
          {#if app.open === r.id && r.bucket !== 'people'}<Inline id={r.id} />{/if}
        {/each}
      </section>
    {/if}

    <p class="foot">
      Hysj only tracks emails where you asked for something or set a date. Deadlines come from your own words (“by Friday”), or default to 3 working
      days.
    </p>
  {/if}
</div>

<style>
  header {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-bottom: 26px;
  }
  .items {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-bottom: 26px;
  }
  .item {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 20px 22px;
    border-radius: 22px;
  }
  .line {
    display: flex;
    align-items: flex-start;
    gap: 14px;
  }
  .av {
    flex: none;
    width: 44px;
    height: 44px;
    border-radius: 50%;
    background: var(--raised);
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 600;
    font-size: 14px;
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .who {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: baseline;
  }
  .sub {
    font-size: 13px;
    color: var(--muted);
  }
  .asked {
    font-size: 17px;
    line-height: 1.4;
  }
  .due {
    font-size: 13px;
    color: var(--secondary);
  }
  .due.late {
    color: var(--accent);
    font-weight: 600;
  }
  .indent {
    padding-left: 58px;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .result {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
    color: var(--ok);
  }
  .draft {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-left: 58px;
    padding: 14px;
    border-radius: 14px;
    background: var(--field);
  }
  .draft label {
    font-size: 13px;
    color: var(--secondary);
  }
  textarea {
    padding: 10px 12px;
    border: 0;
    border-radius: 10px;
    background: var(--surface);
    resize: vertical;
    font: inherit;
    line-height: 1.5;
    color: var(--ink);
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 26px;
  }
  h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 500;
    color: var(--muted);
  }
  .answered {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 16px;
    padding: 10px 0;
    border-bottom: 1px solid var(--raised);
  }
  .answered span {
    flex: 1 1 260px;
  }
  .link {
    padding: 0;
    border: 0;
    background: none;
    font-size: 13px;
    color: var(--secondary);
    text-decoration: underline;
  }
  .foot {
    margin: 0;
    font-size: 13px;
    line-height: 1.6;
    color: var(--muted);
  }
  @media (max-width: 560px) {
    .indent,
    .draft {
      padding-left: 0;
      margin-left: 0;
    }
  }
</style>
