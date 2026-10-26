<script lang="ts">
  import { act, app, copy, toggle } from '../lib/app.svelte.ts';
  import { api } from '../lib/api.ts';
  import { bookingDetail, bookingsHeadline, groupTrips, isTrip, localParts, shortMonth, tripTitle } from '../lib/findit.ts';
  import type { BookingItem } from '../lib/types.ts';
  import Inline from '../components/Inline.svelte';
  import KindFix from '../components/KindFix.svelte';

  const now = Date.now() / 1000;
  let upcoming = $derived(app.data!.bookings.filter((b) => b.start_ts === null || b.start_ts >= now - 12 * 3600));
  let past = $derived(app.data!.bookings.filter((b) => b.start_ts !== null && b.start_ts < now - 12 * 3600).reverse());
  let groups = $derived(groupTrips(upcoming));
  let trips = $derived(groups.filter(isTrip));
  let later = $derived(groups.filter((g) => !isTrip(g)).flat());
  let headline = $derived(app.showPast ? `${past.length} past bookings.` : bookingsHeadline(groups, new Date()));
  const moved = (b: BookingItem) => localParts(b.moved_from)?.time;
</script>

{#snippet row(b: BookingItem)}
  {@const d = localParts(b.start)?.date}
  <div class="booking">
    <div class="date"><span class="day">{d?.getDate() ?? ''}</span><span class="month">{d ? shortMonth(d) : ''}</span></div>
    <div class="what">
      <span class="title"><strong>{b.title}</strong>{#if moved(b)}<span class="chip-warn">Moved from {moved(b)}</span>{/if}</span>
      <span class="detail">{bookingDetail(b)}</span>
      {#if b.pass}<span class="pass-note">Pass attached by {b.provider ?? 'the sender'}</span>{/if}
      {#if b.message}
        <button class="source" onclick={() => toggle(b.message!)}>{b.sources <= 1 ? 'Confirmation email' : `Confirmation + ${b.sources - 1} update${b.sources > 2 ? 's' : ''}`}</button>
        {#if app.open === b.message}<Inline id={b.message} />{/if}
      {/if}
      <KindFix record={b.key} is="booking" />
    </div>
    <button class="ref" aria-label="Copy reference {b.reference}" onclick={() => copy(b.reference, `ref:${b.key}`)}>
      <span class="mono">{b.reference}</span><span class="copy">{app.copied === `ref:${b.key}` ? 'Copied' : 'Copy'}</span>
    </button>
    {#if b.pass}
      {@const pass = b.pass}
      <button class="btn primary wallet" onclick={() => act('Opening in Wallet…', () => api.attachmentSave(pass.id, pass.index, true))}>
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="6" width="18" height="14" rx="3"></rect><path d="M3 10h18M16 14.5h2"></path><path d="M6 6l9-3 1.5 3"></path></svg>
        Add to Wallet
      </button>
    {/if}
  </div>
{/snippet}

{#snippet group(title: string, note: string, items: BookingItem[])}
  <section class="card group">
    <div class="ghead"><h2>{title}</h2><span>{note}</span></div>
    {#each items as b (b.key)}{@render row(b)}{/each}
  </section>
{/snippet}

<div class="column">
  <header>
    <p class="meta">Bookings · from confirmation emails, changes merged in</p>
    <h1 class="h1">{headline}</h1>
  </header>
  {#if app.showPast}
    {#if past.length}{@render group('Past', '', past)}{:else}<p class="empty">No past bookings.</p>{/if}
  {:else if !upcoming.length}
    <p class="empty">Bookings appear when a confirmation email has the details.</p>
  {:else}
    {#each trips as g (g[0].key)}{@render group(tripTitle(g), `${g.length} bookings`, g)}{/each}
    {#if later.length}{@render group(trips.length ? 'Later' : 'Coming up', '', later)}{/if}
  {/if}
</div>

<style>
  .pass-note {
    font-size: 12px;
    color: var(--muted);
  }
  .wallet {
    min-height: 40px;
    gap: 8px;
    padding: 0 14px;
  }
  header {
    margin-bottom: 32px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .group {
    padding: 10px;
    border-radius: 24px;
    box-shadow: var(--shadow-md);
    margin-bottom: 24px;
  }
  .ghead {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 12px;
    padding: 12px 14px 8px;
  }
  .ghead span {
    font-size: 13px;
    color: var(--muted);
  }
  h2 {
    margin: 0;
    font-size: 20px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  .booking {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px 18px;
    padding: 14px;
  }
  .date {
    flex: none;
    width: 52px;
    display: flex;
    flex-direction: column;
    align-items: center;
  }
  .day {
    font-size: 22px;
    font-weight: 600;
    line-height: 1;
  }
  .month {
    font-size: 12px;
    color: var(--muted);
  }
  .what {
    flex: 1 1 260px;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .title {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
    font-size: 16px;
  }
  .chip-warn {
    padding: 2px 8px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 600;
    background: var(--warn-bg);
    color: var(--warn);
  }
  .detail {
    color: var(--secondary);
  }
  .source {
    align-self: flex-start;
    padding: 4px 0;
    border: 0;
    background: none;
    font-size: 13px;
    color: var(--muted);
    text-decoration: underline;
  }
  .ref {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    height: 40px;
    padding: 0 12px;
    border: 0;
    border-radius: 12px;
    background: var(--field);
  }
  .ref .mono {
    font-size: 14px;
    letter-spacing: 0.06em;
  }
  .copy {
    font-size: 12px;
    color: var(--secondary);
  }
</style>
