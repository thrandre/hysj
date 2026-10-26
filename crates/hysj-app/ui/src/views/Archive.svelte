<script lang="ts">
  // Everything that left the Desk, and why; a wrong call is corrected here.
  import { act, app, digestNow, digestSplit, toggle } from '../lib/app.svelte.ts';
  import { api } from '../lib/api.ts';
  import { archive, day, KINDS, time, weekLine, type ArchiveItem, type ArchiveKind } from '../lib/archive.ts';
  import type { MessageSummary } from '../lib/types.ts';
  import Inline from '../components/Inline.svelte';

  /** ponytail: rows drawn at once; search reaches the rest. */
  const SHOWN = 300;

  let q = $state('');
  let filter = $state<ArchiveKind | 'all'>('all');
  let blocked = $state<MessageSummary[]>([]);
  /** Corrections made here, kept in place (with Undo) after the mail moves. */
  let acted = $state<Record<string, { item: ArchiveItem; result: string; undo: () => Promise<unknown> }>>({});

  $effect(() => {
    void app.data;
    api.list('blocked').then((b) => (blocked = b), () => {});
  });

  let all = $derived.by(() => {
    const s = app.data!;
    const items = archive(
      {
        done: s.done,
        delivered: digestSplit()[0],
        blocked,
        codes: s.codes,
        bookings: s.bookings,
        orders: s.orders,
        retentionHours: s.codeSettings.retention_hours,
      },
      digestNow(),
    );
    const here = new Set(items.map((i) => i.id));
    return [...items, ...Object.values(acted).map((a) => a.item).filter((i) => !here.has(i.id))].sort((a, b) => b.ts - a.ts);
  });
  let filters = $derived(
    (['all', 'done', 'sorted', 'blocked', 'expired'] as const).map((id) => ({
      id,
      label: id === 'all' ? 'All' : KINDS[id],
      count: id === 'all' ? all.length : all.filter((i) => i.kind === id).length,
    })),
  );
  let matching = $derived.by(() => {
    const ql = q.trim().toLowerCase();
    return all.filter((i) => (filter === 'all' || i.kind === filter) && (!ql || `${i.from} ${i.subject} ${i.why}`.toLowerCase().includes(ql)));
  });
  let groups = $derived.by(() => {
    const now = digestNow();
    const out: { day: string; items: ArchiveItem[] }[] = [];
    for (const i of matching.slice(0, SHOWN)) {
      const d = day(i.ts, now);
      if (out.at(-1)?.day !== d) out.push({ day: d, items: [] });
      out.at(-1)!.items.push(i);
    }
    return out;
  });

  async function correct(i: ArchiveItem, f: () => Promise<unknown>, result: string, undo: () => Promise<unknown>) {
    if ((await act(null, f)) === undefined) return;
    acted[i.id] = { item: i, result, undo };
  }

  function back(i: ArchiveItem) {
    const desk = i.m!.bucket === 'people';
    void correct(i, () => api.restore([i.id]), desk ? 'Back on your Desk.' : 'Back in the Digest.', () => api.done([i.id]));
  }

  function wrong(i: ArchiveItem) {
    const m = i.m!;
    if (i.kind === 'blocked') {
      void correct(i, () => api.screenerForget(m.from_addr), `Learned: ${m.from} is unblocked and goes back to the Screener.`, () =>
        api.screenerDecide(m.from_addr, 'block'),
      );
      return;
    }
    const before = m.reason.kind === 'decision' ? m.reason.decision : null;
    void correct(i, () => api.screenerDecide(m.from_addr, 'allow'), `Learned: ${m.from} reaches your Desk from now on.`, () =>
      before ? api.screenerDecide(m.from_addr, before) : api.screenerForget(m.from_addr),
    );
  }

  async function undo(id: string) {
    if ((await act(null, acted[id].undo)) === undefined) return;
    delete acted[id];
  }
</script>

<div class="column wide">
  <header>
    <p class="meta">Archive · all accounts · nothing is deleted without asking</p>
    <h1 class="h1">Everything that left your Desk — and why.</h1>
    <p class="lede">{weekLine(all, digestNow())} If one shouldn’t be here, say so and Hysj learns.</p>
  </header>

  <div class="tools">
    <label class="search">
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"
        ><circle cx="11" cy="11" r="7"></circle><path d="M20 20l-3.5-3.5"></path></svg
      >
      <span class="sr">Search the archive</span>
      <input bind:value={q} placeholder="Search the archive — sender, subject or reason" />
    </label>
    <div class="segmented" role="group" aria-label="Filter by reason">
      {#each filters as f (f.id)}
        <button aria-pressed={filter === f.id} onclick={() => (filter = f.id)}>{f.label} <span class="mono count">{f.count}</span></button>
      {/each}
    </div>
  </div>

  {#each groups as g (g.day)}
    <section>
      <h2>{g.day}</h2>
      <div class="card list">
        {#each g.items as i (i.id)}
          {@const a = acted[i.id]}
          <div class="row" class:decided={a}>
            <div class="line">
              <span class="kind {i.kind}">{KINDS[i.kind]}</span>
              <div class="text">
                <span><strong>{i.from}</strong> <span class="subject">{i.subject}</span></span>
                <span class="why">{i.why}</span>
              </div>
              <span class="mono when">{time(i.ts)}</span>
            </div>
            {#if a}
              <div class="actions result">
                <span>{a.result}</span>
                <button class="act" onclick={() => undo(i.id)}>Undo</button>
              </div>
            {:else}
              <div class="actions">
                {#if i.kind === 'done'}
                  <button class="act" onclick={() => back(i)}>{i.m?.bucket === 'people' ? 'Back to Desk' : 'Back to the Digest'}</button>
                {:else if i.kind === 'sorted' || i.kind === 'blocked'}
                  <button class="act" onclick={() => wrong(i)}>{i.kind === 'blocked' ? 'Unblock' : 'Wrong call'}</button>
                {/if}
                <button class="open" aria-expanded={app.open === i.id} onclick={() => toggle(i.id)}>{app.open === i.id ? 'Close' : 'Open'}</button>
              </div>
            {/if}
            {#if app.open === i.id}<Inline id={i.id} />{/if}
          </div>
        {/each}
      </div>
    </section>
  {/each}

  {#if !groups.length}
    <p class="none">
      {q.trim() ? `Nothing in the archive matches “${q.trim()}”. Try the command bar — it also searches your Desk and digests.` : 'Nothing archived yet.'}
    </p>
  {:else if matching.length > SHOWN}
    <p class="foot">Showing the newest {SHOWN} of {matching.length}. Search to find older ones.</p>
  {/if}

  <p class="foot">
    Expired codes disappear for good after {app.data!.codeSettings.retention_hours} hours. Everything else stays here until you delete it — Hysj never
    deletes mail on its own.
  </p>
</div>

<style>
  .wide {
    max-width: 820px;
  }
  header {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-bottom: 24px;
  }
  .tools {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
    margin-bottom: 24px;
  }
  .search {
    flex: 1 1 260px;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 46px;
    padding: 0 14px;
    border-radius: 14px;
    background: var(--surface);
    box-shadow: 0 4px 14px rgba(17, 17, 17, 0.05);
    color: var(--secondary);
  }
  .search input {
    flex: 1;
    min-width: 0;
    height: 40px;
    border: 0;
    background: transparent;
    font: inherit;
    color: var(--ink);
    outline: none;
  }
  .search:focus-within {
    outline: 2px solid var(--ink);
    outline-offset: 2px;
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }
  .segmented {
    display: flex;
    flex-wrap: wrap;
    gap: 2px;
    padding: 3px;
    border-radius: 999px;
    background: var(--raised);
  }
  .segmented button {
    min-height: 34px;
    padding: 0 12px;
    border: 0;
    border-radius: 999px;
    background: transparent;
    font-size: 13px;
  }
  .segmented button[aria-pressed='true'] {
    background: var(--segment);
    box-shadow: 0 1px 3px rgba(17, 17, 17, 0.12);
  }
  .count {
    font-size: 11px;
    color: var(--muted);
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 24px;
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
  }
  .row {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 14px;
    border-radius: 14px;
  }
  .row.decided {
    background: var(--field);
  }
  .line {
    display: flex;
    align-items: flex-start;
    gap: 12px;
  }
  .kind {
    flex: none;
    margin-top: 2px;
    padding: 2px 8px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 600;
    background: var(--raised);
    color: var(--body);
  }
  .kind.sorted {
    background: var(--ok-bg);
    color: var(--ok);
  }
  .kind.blocked {
    background: var(--error-bg);
    color: var(--error);
  }
  .kind.expired {
    background: var(--warn-bg);
    color: var(--warn);
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .subject {
    color: var(--body);
  }
  .why {
    font-size: 13px;
    line-height: 1.5;
    color: var(--secondary);
  }
  .when {
    flex: none;
    font-size: 12px;
    color: var(--muted);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding-left: 4px;
  }
  .result {
    gap: 10px;
    color: var(--ok);
    font-size: 13px;
  }
  .act {
    min-height: 32px;
    padding: 0 12px;
    border: 0;
    border-radius: 999px;
    background: var(--field);
    font-size: 13px;
  }
  .decided .act {
    background: var(--surface);
  }
  .open {
    min-height: 32px;
    padding: 0 10px;
    border: 0;
    background: none;
    font-size: 13px;
    color: var(--secondary);
    text-decoration: underline;
  }
  .none {
    margin: 0 0 24px;
    padding: 32px;
    border-radius: 20px;
    background: var(--surface);
    text-align: center;
    color: var(--secondary);
  }
  .foot {
    margin: 0 0 12px;
    font-size: 13px;
    line-height: 1.6;
    color: var(--muted);
  }
</style>
