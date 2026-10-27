<script lang="ts">
  // The Digest as an edition (Digest mockup): written by the daemon from the
  // delivery's mail; drawn here. Items open in place; photos are grey
  // placeholders (remote images never load here).
  import { act, markDone, app, go, toggle } from '../lib/app.svelte.ts';
  import { api } from '../lib/api.ts';
  import type { Edition, Item, Section, Unsure } from '../lib/types.ts';
  import Inline from '../components/Inline.svelte';

  let edition = $state<Edition | null>(null);
  let more = $state<Record<string, boolean>>({});
  let showPromos = $state(false);
  let refetch = $state(0);
  /** "Not sure about these" answered here: kept in view with the outcome. */
  let answered = $state<{ u: Unsure; result: string }[]>([]);
  let unsure = $derived([
    ...answered,
    ...(edition?.unsure ?? []).filter((u) => !answered.some((a) => a.u.sender === u.sender)).map((u) => ({ u, result: '' })),
  ]);
  const COUNT = ['', 'this one', 'these two', 'these three'];

  async function answer(u: Unsure, through: boolean) {
    const ok = await act(null, () => api.screenerDecide(u.sender, through ? 'allow' : 'digest'));
    if (ok === undefined) return;
    const result = through ? `Got it — mail from ${u.from} will come through right away.` : 'Thanks — kept as it is.';
    answered = [...answered, { u, result }];
  }

  // Refetch when the delivery changes or the daemon reports new mail.
  $effect(() => {
    const pick = app.digestPick;
    void app.data;
    void refetch;
    api.digestEdition(pick).then(
      (e) => {
        edition = e;
      },
      (e) => {
        app.error = String(e);
      },
    );
  });

  const shown = (s: Section, key: string) => (more[key] ? s.items : s.items.slice(0, s.shown));
  // Placeholder tints for listing photos (the mockup's greys).
  const tints = ['#D9D9D3', '#CFD8D1', '#DDD5CA', '#D3D7DA', '#D8D2C6', '#CCD5D7'];
  const tagTone = (t: string) => (/drop|ned/i.test(t) ? 'ok' : /needs|view|visning|frist/i.test(t) ? 'warn' : 'plain');
</script>

{#snippet open(i: Item)}
  {#if app.open === i.id}<Inline id={i.id} />{/if}
{/snippet}

{#snippet tag(t: string | undefined)}
  {#if t}<span class="tag {tagTone(t)}">{t}</span>{/if}
{/snippet}

<article class="column">
  {#if !edition}
    <p class="meta">Loading…</p>
  {:else}
    <header>
      <p class="meta">{edition.meta}</p>
      <h1 class="h1">{edition.headline}</h1>
      {#if edition.lede}<p class="lede big">{edition.lede}</p>{/if}
    </header>

    {#if unsure.length}
      <section class="unsure" aria-labelledby="h-unsure">
        <div class="unsure-head">
          <h2 id="h-unsure">Not sure about {COUNT[unsure.length] ?? 'these'}</h2>
          <span>Hysj held {unsure.length === 1 ? 'it' : 'them'}, but {unsure.length === 1 ? 'it' : 'they'} might matter. Tell it once and it remembers.</span>
        </div>
        {#each unsure as { u, result } (u.sender)}
          <div class="unsure-row">
            <button class="unsure-title" onclick={() => toggle(u.id)}>{u.title}</button>
            <span class="why">{u.why}</span>
            {#if result}
              <span class="result">{result}</span>
            {:else}
              <div class="row">
                <button class="btn small primary" onclick={() => answer(u, true)}>Should have come through</button>
                <button class="btn small" onclick={() => answer(u, false)}>Right call</button>
              </div>
            {/if}
            {#if app.open === u.id}<Inline id={u.id} />{/if}
          </div>
        {/each}
      </section>
    {/if}

    {#each edition.stories as story, si (story.kind)}
      <section aria-labelledby="h-{si}">
        <span class="label">{story.label}</span>
        <h2 id="h-{si}">{story.head}</h2>
        {#if story.kind === 'money'}
          <p class="para">
            Bookings live in <button class="inline-link" onclick={() => go('bookings')}>Bookings</button> and orders in
            <button class="inline-link" onclick={() => go('orders')}>Orders</button> — shipping updates move them along without reaching you.
          </p>
        {:else if story.para}
          <p class="para">{story.para}</p>
        {/if}

        {#each story.sections as sec, k (k)}
          {@const key = `${si}-${k}`}
          {#if sec.layout === 'cards'}
            <div class="cards">
              {#each shown(sec, key) as i, n (i.id + n)}
                <button class="tile card" onclick={() => (i.link ? api.openLink(i.link) : toggle(i.id))}>
                  {#if i.image}
                    <img class="photo" src={i.image} alt="" loading="lazy" referrerpolicy="no-referrer" />
                  {:else}
                    <span class="photo" style:background={tints[n % tints.length]} role="img" aria-label="Listing photo"></span>
                  {/if}
                  <span class="tile-text">
                    {@render tag(i.tag)}
                    <strong>{i.title}</strong>
                    <span class="sub">{i.sub}</span>
                    {#if i.meta}<span class="mono price">{i.meta}</span>{/if}
                  </span>
                </button>
              {/each}
            </div>
          {:else if sec.layout === 'reading'}
            <div class="reading">
              {#each shown(sec, key) as i (i.id)}
                <button class="read" onclick={() => toggle(i.id)}>
                  <span class="sub">{i.sub}</span>
                  <span class="title">{i.title}</span>
                  {#if i.text}<span class="gist">{i.text}</span>{/if}
                </button>
                {@render open(i)}
              {/each}
            </div>
          {:else if sec.layout === 'lines'}
            <div class="lines">
              {#each shown(sec, key) as i (i.id)}
                <button class="line" onclick={() => toggle(i.id)}>
                  <strong>{i.title}</strong> <span class="dim">—</span> {i.sub}
                  {#if i.meta}<span class="dim"> · {i.meta}</span>{/if}
                  {@render tag(i.tag)}
                </button>
                {@render open(i)}
              {/each}
            </div>
          {:else}
            <div class="card list" class:compact={sec.layout === 'compact'}>
              {#if sec.title}<span class="list-title">{sec.title}</span>{/if}
              {#each shown(sec, key) as i, n (i.id + n)}
                <button class="row" onclick={() => (i.link ? api.openLink(i.link) : toggle(i.id))}>
                  {#if sec.layout === 'compact'}
                    {#if i.image}
                      <img class="thumb" src={i.image} alt="" loading="lazy" referrerpolicy="no-referrer" />
                    {:else}
                      <span class="thumb" style:background={tints[n % tints.length]} role="img" aria-label="Listing photo"></span>
                    {/if}
                  {/if}
                  <span class="what"><strong>{i.title}</strong><span class="sub">{i.sub}</span></span>
                  {@render tag(i.tag)}
                  <span class="mono meta-r">{i.meta}</span>
                </button>
                {@render open(i)}
              {/each}
              {#if sec.items.length > sec.shown}
                <button class="more" onclick={() => (more[key] = !more[key])}>
                  {more[key] ? 'Show fewer' : `Show ${sec.items.length - sec.shown} more${sec.title ? ` ${sec.title.toLowerCase()}` : ''}`}
                </button>
              {/if}
            </div>
          {/if}
          {#if sec.layout !== 'list' && sec.layout !== 'compact' && sec.items.length > sec.shown}
            <button class="more bare" onclick={() => (more[key] = !more[key])}>
              {more[key] ? 'Show fewer' : `Show ${sec.items.length - sec.shown} more${sec.title ? ` ${sec.title.toLowerCase()}` : ''}`}
            </button>
          {/if}
        {/each}
        {#if story.kind === 'listings' && story.sections.some((x) => x.layout === 'cards' || x.layout === 'compact') && !story.sections.some((x) => x.items.some((i) => i.image))}
          <p class="photos">
            Photos aren’t loaded (loading them tells FINN you opened this).
            <button class="inline-link" onclick={() => api.senderImages('agent@finn.no', true).then(() => refetch++)}>Show photos from FINN</button>
          </p>
        {/if}
        {#if story.kind === 'listings' && edition.controls.length}
          <div class="controls">
            <span>Getting too many?</span>
            {#each edition.controls as c (c.key)}
              <button onclick={() => act(null, () => api.digestSet(c.key, c.on)).then(() => refetch++)}>{c.label}</button>
            {/each}
          </div>
        {/if}
      </section>
    {/each}

    <footer class="card">
      <p class="end">{edition.ending.text}</p>
      {#if edition.ending.note}
        <p class="note">
          {edition.ending.note}
          <button class="inline-link" onclick={() => (showPromos = !showPromos)}>{showPromos ? 'Hide them' : 'Show them'}</button>
        </p>
        {#if showPromos}
          <div class="lines">
            {#each edition.ending.promotions as i (i.id)}
              <button class="line" onclick={() => toggle(i.id)}><strong>{i.title}</strong> <span class="dim">—</span> {i.sub} <span class="dim">· {i.meta}</span></button>
              {@render open(i)}
            {/each}
          </div>
        {/if}
      {/if}
      {#if edition.ids.length}
        <button class="btn primary" onclick={() => markDone(edition!.ids)}>Mark as read</button>
      {/if}
    </footer>
  {/if}
</article>

<style>
  .unsure {
    gap: 10px;
    padding: 20px;
    border-radius: 20px;
    background: var(--surface);
    box-shadow: var(--shadow-sm);
  }
  .unsure-head {
    display: flex;
    flex-direction: column;
    gap: 4px;
    color: var(--secondary);
  }
  .unsure-head h2 {
    margin: 0;
    font-size: 19px;
    letter-spacing: -0.01em;
    color: var(--ink);
  }
  .unsure-row {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    padding: 12px 0;
    border-top: 1px solid var(--line);
  }
  .unsure-title {
    padding: 0;
    border: 0;
    background: none;
    font: inherit;
    font-weight: 600;
    color: var(--ink);
    text-align: left;
    cursor: pointer;
  }
  .unsure-row .why {
    font-size: 13px;
    color: var(--secondary);
  }
  .unsure-row .row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .result {
    color: var(--ok);
  }
  header {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-bottom: 44px;
  }
  .big {
    font-size: 18px;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 44px;
  }
  .label {
    font-size: 13px;
    color: var(--muted);
  }
  h2 {
    margin: 0 0 6px;
    font-size: 23px;
    line-height: 1.3;
    font-weight: 600;
    letter-spacing: -0.015em;
  }
  .para {
    margin: 0 0 10px;
    font-size: 16px;
    line-height: 1.6;
    color: var(--body);
  }
  .sub {
    color: var(--secondary);
  }
  .dim {
    color: var(--muted);
  }
  button {
    font: inherit;
    color: inherit;
  }
  /* Photo tiles. */
  .cards {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    margin-bottom: 6px;
  }
  .tile {
    /* Three to a row, as on the board, whatever the count. */
    flex: 0 1 calc((100% - 24px) / 3);
    min-width: 200px;
    min-width: 0;
    display: flex;
    flex-direction: column;
    padding: 0;
    border: 0;
    border-radius: 18px;
    overflow: hidden;
    text-align: left;
  }
  .photo {
    display: block;
    width: 100%;
    aspect-ratio: 16 / 10;
    object-fit: cover;
  }
  .tile strong {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .tile .sub,
  .row .sub {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .photos {
    margin: 4px 0 0;
    font-size: 13px;
    color: var(--muted);
  }
  .tile-text {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 12px 14px 14px;
  }
  .price {
    font-size: 13px;
  }
  /* Lists. */
  .list {
    display: flex;
    flex-direction: column;
    padding: 6px;
    border-radius: 18px;
    margin-bottom: 6px;
  }
  .list-title {
    padding: 10px 12px 4px;
    font-size: 13px;
    color: var(--muted);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 14px;
    min-height: 52px;
    padding: 8px 12px;
    border: 0;
    border-radius: 12px;
    background: transparent;
    text-align: left;
  }
  .row:hover,
  .read:hover,
  .line:hover {
    background: color-mix(in srgb, var(--field) 70%, transparent);
  }
  .what {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .meta-r {
    font-size: 12px;
    color: var(--muted);
  }
  .compact .meta-r {
    font-size: 13px;
    color: var(--ink);
  }
  .thumb {
    object-fit: cover;
    flex: none;
    width: 60px;
    height: 42px;
    border-radius: 8px;
  }
  .more {
    min-height: 44px;
    margin-top: 2px;
    border: 0;
    border-radius: 12px;
    background: var(--field);
    font-weight: 500;
  }
  /* Under cards, on the ground: needs its own fill to read as a button. */
  .more.bare {
    background: var(--raised);
  }
  .controls {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: 6px;
    font-size: 13px;
    color: var(--secondary);
  }
  .controls button {
    min-height: 32px;
    padding: 0 12px;
    border: 0;
    border-radius: 999px;
    background: var(--raised);
    font: inherit;
    font-size: 13px;
    color: var(--ink);
    cursor: pointer;
  }
  /* Reading. */
  .read {
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: calc(100% + 32px);
    padding: 14px 16px;
    margin: 0 -16px;
    border: 0;
    border-radius: 16px;
    background: transparent;
    text-align: left;
  }
  .read .sub {
    font-size: 13px;
    color: var(--muted);
  }
  .title {
    font-size: 17px;
    font-weight: 600;
  }
  .gist {
    font-size: 15px;
    line-height: 1.55;
    color: var(--body);
  }
  /* Sender lines. */
  .lines {
    display: flex;
    flex-direction: column;
  }
  .line {
    padding: 10px 16px;
    margin: 0 -16px;
    border: 0;
    border-radius: 12px;
    background: transparent;
    text-align: left;
    font-size: 16px;
    line-height: 1.5;
    color: var(--body);
  }
  .line strong {
    color: var(--ink);
  }
  .tag {
    align-self: flex-start;
    display: inline-block;
    margin-left: 6px;
    padding: 2px 8px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 600;
    background: var(--raised);
    color: var(--body);
    vertical-align: 1px;
  }
  .tile .tag {
    margin-left: 0;
  }
  .tag.ok {
    background: var(--ok-bg);
    color: var(--ok);
  }
  .tag.warn {
    background: var(--warn-bg);
    color: var(--warn);
  }
  .inline-link {
    padding: 4px 2px;
    margin: -4px -2px;
    border: 0;
    background: none;
    text-decoration: underline;
    text-underline-offset: 3px;
  }
  footer {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 14px;
    padding: 22px;
  }
  .end {
    margin: 0;
    font-size: 17px;
    font-weight: 500;
  }
  .note {
    margin: 0;
    line-height: 1.6;
    color: var(--secondary);
  }
  footer .lines {
    align-self: stretch;
  }
</style>
