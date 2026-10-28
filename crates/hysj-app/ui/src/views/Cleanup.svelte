<script lang="ts">
  // Clean up: unsubscribe at the source, least read first. Unsubscribing waits
  // a few seconds first, so Undo really undoes (it can't be taken back once
  // it's sent).
  import { act, app, go } from '../lib/app.svelte.ts';
  import { api } from '../lib/api.ts';
  import { shortMonth } from '../lib/findit.ts';
  import type { Cleanup, Subscription } from '../lib/types.ts';

  /** Seconds before an unsubscribe goes out. */
  const GRACE = 10;

  let data = $state<Cleanup | null>(null);
  /** Unsubscribes waiting to go out, by address. */
  let pending = $state<Record<string, ReturnType<typeof setTimeout>>>({});
  /** How each was done here ("one_click", "mailto", "web"). */
  let how = $state<Record<string, string>>({});
  let bulk = $state<'idle' | 'pending' | 'done'>('idle');
  let bulkCount = $state(0);
  let bulkTimer: ReturnType<typeof setTimeout> | undefined;
  let pileDone = $state(false);

  function load() {
    api.cleanup().then(
      (c) => (data = c),
      (e) => (app.error = String(e)),
    );
  }
  $effect(() => {
    void app.data;
    load();
  });

  let subs = $derived(data?.subscriptions ?? []);
  let live = $derived(subs.filter((s) => s.state === 'active' || s.state === 'kept'));
  let never = $derived(subs.filter((s) => s.never && s.state === 'active' && s.method !== 'none' && !pending[s.address]));
  let learning = $derived(!subs.some((s) => s.counted >= 4));
  let read = $derived(live.filter((s) => s.counted > 0 && s.opened / s.counted > 0.5).length);
  let since = $derived.by(() => {
    const d = new Date((data?.counting_since ?? 0) * 1000);
    return `${d.getDate()} ${shortMonth(d)}`;
  });
  let headline = $derived(
    !live.length
      ? 'No newsletters left to clean up.'
      : learning
        ? `You get ${live.length} ${live.length === 1 ? 'newsletter' : 'newsletters'}. Hysj is still learning which ones you read.`
        : `You get ${live.length} ${live.length === 1 ? 'newsletter' : 'newsletters'} and read ${read} of them.`,
  );

  async function send(addresses: string[]): Promise<number> {
    let n = 0;
    for (const a of addresses) {
      const r = await act(addresses.length === 1 ? 'Unsubscribing…' : `Unsubscribing from ${addresses.length}…`, () => api.unsubscribe(a));
      if (r) {
        how[a] = r.method;
        n++;
      }
    }
    load();
    return n;
  }

  function unsubscribe(s: Subscription) {
    pending[s.address] = setTimeout(() => {
      delete pending[s.address];
      void send([s.address]);
    }, GRACE * 1000);
  }
  function undo(s: Subscription) {
    clearTimeout(pending[s.address]);
    delete pending[s.address];
  }

  function unsubscribeAll() {
    const addresses = never.map((s) => s.address);
    bulkCount = addresses.length;
    bulk = 'pending';
    bulkTimer = setTimeout(async () => {
      bulkCount = await send(addresses);
      bulk = 'done';
    }, GRACE * 1000);
  }
  function undoAll() {
    clearTimeout(bulkTimer);
    bulk = 'idle';
  }

  async function keep(s: Subscription, on: boolean) {
    if ((await act(null, () => api.cleanupKeep(s.address, on))) !== undefined) load();
  }

  async function archivePile() {
    if ((await act('Archiving the old pile…', () => api.pileArchive())) === undefined) return;
    pileDone = true;
    load();
  }

  const settled = (s: Subscription) =>
    s.state === 'kept'
      ? 'Kept — Hysj won’t suggest this again'
      : s.state === 'blocked'
        ? 'Kept sending after you unsubscribed — blocked'
        : ({ one_click: 'Unsubscribed · one click', mailto: 'Unsubscribed · email sent', web: 'Their unsubscribe page opened in your browser' }[how[s.address] ?? s.method] ??
          'Unsubscribed');
  const reads = (s: Subscription) =>
    s.counted === 0 ? `counting since ${since}` : s.never ? 'never opened' : `opened ${s.opened} of ${s.counted}`;
  const perWeek = $derived(Math.round(never.reduce((a, s) => a + s.per_week, 0)));
</script>

<div class="column">
  <header>
    <p class="meta">Clean up · last 90 days, all accounts</p>
    {#if data}
      <h1 class="h1">{headline}</h1>
      <p class="lede">
        Hiding noise is good. Not getting it at all is better. Unsubscribing happens directly with the sender — no web pages, no “are you sure” forms,
        where the sender allows it.
      </p>
    {:else}
      <h1 class="h1">Looking at your subscriptions…</h1>
    {/if}
  </header>

  {#if data}
    {#if bulk === 'pending'}
      <div class="status" role="status">
        <span>Unsubscribing from {bulkCount} {bulkCount === 1 ? 'sender' : 'senders'} in a few seconds.</span>
        <button class="btn small" onclick={undoAll}>Undo</button>
      </div>
    {:else if bulk === 'done'}
      <div class="status" role="status">
        <span>Unsubscribed from {bulkCount} {bulkCount === 1 ? 'sender' : 'senders'}. If any keeps sending after 7 days, Hysj blocks it and tells you.</span>
      </div>
    {:else if never.length}
      <div class="bulk card">
        <div class="bulk-text">
          <span class="bulk-title">Unsubscribe from the {never.length} you never open</span>
          <span class="sub">You haven’t opened a single email from them since {since}. That’s about {perWeek} {perWeek === 1 ? 'email' : 'emails'} a week gone.</span>
        </div>
        <button class="btn primary" onclick={unsubscribeAll}>Unsubscribe from {never.length}</button>
      </div>
    {/if}

    {#if subs.length}
      <section aria-labelledby="h-subs">
        <h2 id="h-subs">Your subscriptions, least read first</h2>
        <div class="card list">
          {#each subs as s (s.address)}
            {@const done = s.state !== 'active'}
            <div class="row">
              <div class="who">
                <span class="name" class:gone={s.state === 'unsubscribed' || s.state === 'blocked'}>{s.name}</span>
                <span class="sub">{s.frequency} · {reads(s)}</span>
              </div>
              <div class="bar" aria-hidden="true"><div style="width: {s.counted ? Math.round((s.opened / s.counted) * 100) : 0}%"></div></div>
              {#if pending[s.address]}
                <span class="state">Unsubscribing…</span>
                <button class="btn small" onclick={() => undo(s)}>Undo</button>
              {:else if done}
                <span class="state" class:ok={s.state === 'unsubscribed'}>{settled(s)}</span>
                {#if s.state === 'kept'}<button class="btn small ghost" onclick={() => keep(s, false)}>Undo</button>{/if}
              {:else}
                <div class="actions">
                  {#if s.method !== 'none'}
                    <button class="btn small" class:primary={s.never} onclick={() => unsubscribe(s)}>Unsubscribe</button>
                  {/if}
                  <button class="btn small" onclick={() => keep(s, true)}>Keep</button>
                </div>
              {/if}
            </div>
          {/each}
        </div>
        <p class="foot">
          “One click” means the sender supports instant unsubscribe. Some only take an email, sent from the address they wrote to; the rest open their
          own page in your browser. Opens are counted in Hysj only, since {since}.
        </p>
      </section>
    {:else}
      <p class="empty">No newsletters in the last 90 days.</p>
    {/if}

    {#if data.pile || pileDone}
      <section class="pile card" aria-labelledby="h-pile">
        <h2 id="h-pile" class="pile-title">The old pile</h2>
        {#if pileDone}
          <span class="ok">Archived, each with its reason. <button class="inline" onclick={() => go('done')}>See them in the Archive</button>.</span>
        {:else}
          <p>
            {data.pile.toLocaleString('en-GB')}
            {data.pile === 1 ? 'email older than a year is a newsletter, notification or receipt' : 'emails older than a year are newsletters, notifications and receipts'}
            you never touched. Archive them — nothing is deleted, and search still finds them.
          </p>
          <div class="actions">
            <button class="btn primary" onclick={archivePile}>Archive {data.pile.toLocaleString('en-GB')} {data.pile === 1 ? 'email' : 'emails'}</button>
            <button class="btn" onclick={() => go('done')}>Look first</button>
          </div>
        {/if}
      </section>
    {/if}
  {/if}
</div>

<style>
  header {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-bottom: 26px;
  }
  .status {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
    margin-bottom: 26px;
    padding: 16px 18px;
    border-radius: 16px;
    background: var(--ok-bg);
    color: var(--ok);
  }
  .status span {
    flex: 1 1 280px;
  }
  .status .btn {
    background: var(--surface);
  }
  .bulk {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 14px;
    margin-bottom: 26px;
    padding: 22px;
    border-radius: 22px;
  }
  .bulk-text {
    flex: 1 1 300px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .bulk-title,
  .pile-title {
    margin: 0;
    font-size: 19px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 26px;
  }
  h2 {
    margin: 0;
    padding: 0 4px;
    font-size: 13px;
    font-weight: 500;
    color: var(--muted);
  }
  .list {
    display: flex;
    flex-direction: column;
    padding: 6px;
    border-radius: 20px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 16px;
    padding: 12px 14px;
    border-radius: 14px;
  }
  .who {
    flex: 1 1 220px;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .name {
    font-weight: 600;
  }
  .name.gone {
    text-decoration: line-through;
    color: var(--muted);
  }
  .sub {
    font-size: 13px;
    color: var(--secondary);
  }
  .bar {
    width: 120px;
    height: 6px;
    border-radius: 3px;
    background: var(--field);
    overflow: hidden;
  }
  .bar div {
    height: 100%;
    background: var(--ink);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .state {
    font-size: 13px;
    color: var(--secondary);
  }
  .ok {
    color: var(--ok);
  }
  .foot {
    margin: 0;
    padding: 0 4px;
    font-size: 13px;
    line-height: 1.6;
    color: var(--muted);
  }
  .pile {
    gap: 12px;
    padding: 22px;
    border-radius: 22px;
  }
  .pile h2 {
    padding: 0;
    color: var(--ink);
  }
  .pile p {
    margin: 0;
    line-height: 1.6;
    color: var(--body);
  }
  .inline {
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    text-decoration: underline;
    font: inherit;
  }
</style>
