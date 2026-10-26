<script lang="ts">
  // Sent: rows that open in place.
  import { app, toggle, visible } from '../lib/app.svelte.ts';
  import { shortMonth } from '../lib/findit.ts';
  import { hhmm, subject } from '../lib/words.ts';
  import Inline from '../components/Inline.svelte';

  let items = $derived(visible(app.data!.sent));
  const when = (ts: number | null) => (ts ? `${new Date(ts * 1000).getDate()} ${shortMonth(new Date(ts * 1000))} ${hhmm(new Date(ts * 1000))}` : '');
</script>

<div class="column">
  <header>
    <p class="meta">Sent · newest first</p>
    <h1 class="h1">What you sent.</h1>
  </header>
  {#if items.length}
    <div class="card list">
      {#each items as m (m.id)}
        <div class="row">
          <button class="open" onclick={() => toggle(m.id)}>
            <strong>To {m.to ?? ''}</strong>
            <span>{subject(m.subject)}</span>
          </button>
          <span class="mono when">{when(m.ts)}</span>
        </div>
        {#if app.open === m.id}<div class="pad"><Inline id={m.id} /></div>{/if}
      {/each}
    </div>
  {:else}
    <p class="empty">Nothing sent yet.</p>
  {/if}
</div>

<style>
  header {
    margin-bottom: 28px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .list {
    padding: 6px;
    border-radius: 22px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding-right: 12px;
  }
  .open {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 12px 16px;
    border: 0;
    border-radius: 16px;
    background: transparent;
    text-align: left;
  }
  .open:hover {
    background: var(--field);
  }
  .open span {
    color: var(--secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .when {
    font-size: 12px;
    color: var(--muted);
    white-space: nowrap;
  }
  .pad {
    padding: 0 16px;
  }
</style>
