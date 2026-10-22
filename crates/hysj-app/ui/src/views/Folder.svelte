<script lang="ts">
  // Sent and Done: rows that open in place; Done can put a message back.
  import { act, app, toggle, visible } from '../lib/app.svelte.ts';
  import { api } from '../lib/api.ts';
  import { shortMonth } from '../lib/findit.ts';
  import { hhmm, subject } from '../lib/words.ts';
  import Inline from '../components/Inline.svelte';

  let { folder }: { folder: 'sent' | 'done' } = $props();
  let items = $derived(visible(folder === 'sent' ? app.data!.sent : app.data!.done));
  const when = (ts: number | null) => (ts ? `${new Date(ts * 1000).getDate()} ${shortMonth(new Date(ts * 1000))} ${hhmm(new Date(ts * 1000))}` : '');
</script>

<div class="column">
  <header>
    {#if folder === 'sent'}
      <p class="meta">Sent · newest first</p>
      <h1 class="h1">What you sent.</h1>
    {:else}
      <p class="meta">Done</p>
      <h1 class="h1">Cleared from the Desk and the Digest.</h1>
      <p class="lede">Restore puts a message back where it was.</p>
    {/if}
  </header>
  {#if items.length}
    <div class="card list">
      {#each items as m (m.id)}
        <div class="row">
          <button class="open" onclick={() => toggle(m.id)}>
            <strong>{folder === 'sent' ? `To ${m.to ?? ''}` : m.from}</strong>
            <span>{subject(m.subject)}</span>
          </button>
          {#if folder === 'done'}<button class="btn small" onclick={() => act(null, () => api.restore([m.id]))}>Restore</button>{/if}
          <span class="mono when">{when(m.ts)}</span>
        </div>
        {#if app.open === m.id}<div class="pad"><Inline id={m.id} /></div>{/if}
      {/each}
    </div>
  {:else}
    <p class="empty">{folder === 'sent' ? 'Nothing sent yet.' : 'Nothing cleared yet.'}</p>
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
