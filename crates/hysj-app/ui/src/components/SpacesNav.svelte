<script lang="ts">
  import { app, go, nextDigest, type View } from '../lib/app.svelte.ts';

  let items = $derived.by(() => {
    const s = app.data!;
    const now = Date.now() / 1000;
    const senders = new Set(s.screener.map((m) => m.from_addr.toLowerCase()));
    return [
      ['desk', 'People', String(s.people.filter((m) => m.intent === 'reply').length)],
      ['digest', 'Digest', nextDigest()],
      ['screener', 'Screener', String(senders.size)],
      ['codes', 'Codes', String(s.codes.filter((c) => c.expires > now).length)],
      ['bookings', 'Bookings', String(s.bookings.filter((b) => b.start_ts === null || b.start_ts >= now - 12 * 3600).length)],
      ['orders', 'Orders', String(s.orders.filter((o) => o.status < 4).length)],
    ] as [View, string, string][];
  });
</script>

<nav aria-label="Spaces">
  {#each items as [view, label, count] (view)}
    <button aria-current={app.view === view ? 'page' : undefined} onclick={() => go(view)}>
      <span>{label}</span><span class="count">{count}</span>
    </button>
  {/each}
</nav>

<style>
  nav {
    position: fixed;
    left: 50%;
    bottom: 24px;
    transform: translateX(-50%);
    z-index: 10;
    display: flex;
    gap: 4px;
    padding: 6px;
    border-radius: 999px;
    background: var(--nav);
    box-shadow: var(--shadow-nav);
    max-width: calc(100vw - 32px);
    overflow-x: auto;
  }
  button {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-height: 44px;
    padding: 0 16px;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--on-nav);
    font-size: 14px;
    white-space: nowrap;
  }
  button[aria-current='page'] {
    background: var(--nav-on);
  }
  .count {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--nav-count);
  }
</style>
