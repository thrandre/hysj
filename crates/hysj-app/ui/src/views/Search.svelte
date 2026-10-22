<script lang="ts">
  // All mail on the server, older mail included (Gmail search syntax works:
  // from:, subject:, before:, has:attachment). Results aren't kept; opening one
  // fetches it from the server.
  import { app, toggle } from '../lib/app.svelte.ts';
  import { api } from '../lib/api.ts';
  import { shortMonth } from '../lib/findit.ts';
  import type { Found } from '../lib/types.ts';
  import { subject } from '../lib/words.ts';
  import Inline from '../components/Inline.svelte';

  let found = $state<Found[] | null>(null);
  let query = $state(app.searchQuery);

  async function run() {
    found = null;
    app.searchQuery = query.trim();
    try {
      found = await api.search(query.trim());
    } catch (e) {
      app.error = String(e);
      found = [];
    }
  }
  run();
  const when = (ts: bigint | number | null) => {
    if (ts == null) return '';
    const d = new Date(Number(ts) * 1000);
    return `${d.getDate()} ${shortMonth(d)} ${d.getFullYear()}`;
  };
</script>

<div class="column">
  <header>
    <p class="meta">Search · all mail on the server</p>
    <form
      onsubmit={(e) => {
        e.preventDefault();
        run();
      }}
    >
      <label class="sr-only" for="search">Search all mail</label>
      <input id="search" bind:value={query} placeholder="from:aisha contract, subject:kvittering before:2025/01/01" spellcheck="false" />
      <button class="btn primary">Search</button>
    </form>
    <p class="hint">Gmail search works here: <span class="mono">from:</span>, <span class="mono">subject:</span>, <span class="mono">before:2024/12/31</span>, <span class="mono">has:attachment</span>.</p>
  </header>

  {#if found === null}
    <p class="hint">Searching…</p>
  {:else if found.length === 0}
    <p class="empty">Nothing on the server matches “{app.searchQuery}”.</p>
  {:else}
    <p class="hint">{found.length === 40 ? 'The 40 newest matches' : `${found.length} found`}</p>
    <div class="card list">
      {#each found as m (m.id)}
        <button class="row" onclick={() => toggle(m.id)}>
          <span class="what"><strong>{m.from}</strong><span>{subject(m.subject)}</span></span>
          <span class="mono when">{when(m.ts)}</span>
        </button>
        {#if app.open === m.id}<div class="pad"><Inline id={m.id} /></div>{/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  header {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-bottom: 24px;
  }
  form {
    display: flex;
    gap: 8px;
  }
  input {
    flex: 1;
    min-width: 0;
    height: 52px;
    padding: 0 16px;
    border: 0;
    border-radius: 14px;
    background: var(--surface);
    box-shadow: var(--shadow-sm);
    font-size: 17px;
    user-select: text;
    -webkit-user-select: text;
  }
  .btn {
    height: 52px;
  }
  .hint {
    margin: 0;
    font-size: 13px;
    color: var(--muted);
  }
  .list {
    margin-top: 10px;
    padding: 6px;
    border-radius: 22px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 12px 16px;
    border: 0;
    border-radius: 16px;
    background: transparent;
    text-align: left;
  }
  .row:hover {
    background: var(--field);
  }
  .what {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .what span {
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
