<script lang="ts">
  import { app, toggle } from '../lib/app.svelte.ts';
  import { api } from '../lib/api.ts';
  import { etaText, localParts, ordersHeadline, shortMonth } from '../lib/findit.ts';
  import Inline from '../components/Inline.svelte';
  import KindFix from '../components/KindFix.svelte';

  const now = Date.now() / 1000;
  const today = new Date();
  let shown = $derived(
    app.data!.orders.filter((o) => (app.orderTab === 0 ? o.status < 4 : app.orderTab === 1 ? o.status === 4 : o.return_by !== null && o.return_by > now)),
  );
  const steps = ['Ordered', 'Shipped', 'Out for delivery', 'Delivered'];
  const day = (d: Date) => `${d.getDate()} ${shortMonth(d)}`;
</script>

<div class="column">
  <header>
    <p class="meta">Orders · confirmations, shipping and receipts in one place</p>
    <h1 class="h1">{ordersHeadline(app.orderTab, shown, today)}</h1>
  </header>
  <div class="list">
    {#each shown as o (o.key)}
      {@const eta = etaText(o, today)}
      {@const ordered = localParts(o.ordered)?.date}
      <div class="card order">
        <div class="top">
          <div class="left">
            <span class="fine">{o.store}{ordered ? ` · ordered ${day(ordered)}` : ''} · {o.sources === 1 ? '1 email' : `${o.sources} emails`}</span>
            <span class="items">{o.items.length ? o.items.join(', ') : `Order ${o.number}`}</span>
          </div>
          <div class="right">
            <span class="eta" class:soon={eta.endsWith('today')}>{eta}</span>
            {#if o.total}<span class="mono total">{o.total}</span>{/if}
          </div>
        </div>
        <ol class="progress" aria-label="Delivery progress">
          {#each steps as s, i (s)}
            <li class:done={i + 1 <= o.status} class:now={i + 1 === o.status}><span class="bar"></span>{s}</li>
          {/each}
        </ol>
        <div class="actions">
          {#if o.tracking_url}<button class="btn small" onclick={() => api.openLink(o.tracking_url!)}>Track</button>{/if}
          {#if o.receipt}<button class="btn small" onclick={() => toggle(o.receipt!)}>Receipt</button>{/if}
          {#if o.return_by && o.return_by > now}<span class="return">Return by {day(new Date(o.return_by * 1000))}</span>{/if}
          <KindFix record={o.key} is="order" />
        </div>
        {#if o.receipt && app.open === o.receipt}<Inline id={o.receipt} />{/if}
      </div>
    {:else}
      <p class="empty">{app.data!.orders.length ? 'Nothing here.' : 'Orders appear when a store’s emails include order details.'}</p>
    {/each}
  </div>
</div>

<style>
  header {
    margin-bottom: 28px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .order {
    display: flex;
    flex-direction: column;
    gap: 18px;
    padding: 22px;
    border-radius: 22px;
  }
  .top {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: 8px 16px;
  }
  .left,
  .right {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .right {
    align-items: flex-end;
  }
  .fine {
    font-size: 13px;
    color: var(--muted);
  }
  .items {
    font-size: 18px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  .eta {
    font-weight: 600;
  }
  .eta.soon {
    color: var(--accent);
  }
  .total {
    font-size: 13px;
    color: var(--secondary);
  }
  .progress {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 6px;
  }
  .progress li {
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: 12px;
    color: var(--muted);
  }
  .progress .bar {
    height: 4px;
    border-radius: 2px;
    background: var(--raised);
  }
  .progress li.done .bar {
    background: var(--ink);
  }
  .progress li.now {
    color: var(--ink);
    font-weight: 600;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }
  .return {
    margin-left: auto;
    padding: 3px 10px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 600;
    background: var(--warn-bg);
    color: var(--warn);
  }
</style>
