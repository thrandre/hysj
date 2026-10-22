<script lang="ts">
  import { app, perform } from '../lib/app.svelte.ts';
  import { hits, matches, suggestions } from '../lib/hits.ts';
  import { firstName } from '../lib/words.ts';

  let query = $state('');
  let at = $state(0);
  let input: HTMLInputElement | undefined = $state();

  const now = () => Date.now() / 1000;
  // What's loaded first; the last line always searches all mail on the server.
  let results = $derived(
    query.trim() && app.data
      ? [
          ...hits(app.data, now())
            .filter((h) => matches(query, `${h.kind} ${h.label} ${h.hint}`))
            .slice(0, 5),
          { kind: 'Search', label: `Search all mail for “${query.trim()}”`, hint: 'older mail on the server too', go: { search: query.trim() } },
        ]
      : [],
  );
  let chips = $derived(app.data ? suggestions(app.data, now()) : []);
  let waiting = $derived(app.data?.people.find((m) => m.intent === 'reply'));
  let placeholder = $derived(
    waiting
      ? `Ask or do anything — “reply to ${firstName(waiting.from)}”, “latest login code”`
      : 'Ask or do anything — “latest login code”, “write to …”',
  );

  $effect(() => {
    if (app.focusCommand) input?.focus();
  });
  $effect(() => {
    void query;
    at = 0;
  });

  function run(i: number) {
    const h = results[i];
    if (!h) return;
    query = '';
    perform(h.go);
  }

  function keys(e: KeyboardEvent) {
    if (e.key === 'Enter') run(at);
    else if (e.key === 'ArrowDown') at = Math.min(at + 1, results.length - 1);
    else if (e.key === 'ArrowUp') at = Math.max(at - 1, 0);
    else if (e.key === 'Escape') {
      query = '';
      input?.blur();
    } else return;
    e.preventDefault();
  }

  /** While typing, results replace the rest of the Desk. */
  export function searching() {
    return query.trim().length > 0;
  }
</script>

<div class="bar">
  <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"
    ><circle cx="11" cy="11" r="7"></circle><path d="M20 20l-3.5-3.5"></path></svg
  >
  <label class="sr-only" for="cmd">Ask or do anything</label>
  <input
    id="cmd"
    bind:this={input}
    bind:value={query}
    {placeholder}
    autocomplete="off"
    spellcheck="false"
    role="combobox"
    aria-expanded={results.length > 0}
    aria-controls="cmd-results"
    onkeydown={keys}
  />
  <kbd class="mono">⌘K</kbd>
</div>

{#if !query.trim()}
  {#if chips.length}
    <div class="chips">
      {#each chips as [label, fill] (label)}
        <button class="chip" onclick={() => ((query = fill), input?.focus())}>{label}</button>
      {/each}
    </div>
  {/if}
{:else}
  <div class="results card" id="cmd-results" role="listbox" aria-label="Results">
    {#each results as h, i (h.kind + h.label + i)}
      <button class="hit" class:at={i === at} role="option" aria-selected={i === at} onclick={() => run(i)} onmouseenter={() => (at = i)}>
        <span class="kind">{h.kind}</span>
        <span class="what"><span class="label">{h.label}</span><span class="hint">{h.hint}</span></span>
        {#if i === at}<span class="key mono">↵</span>{/if}
      </button>
    {:else}
      <p class="none">Nothing in your loaded mail matches “{query.trim()}”.</p>
    {/each}
  </div>
{/if}

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 14px;
    height: 64px;
    padding: 0 20px;
    border-radius: 18px;
    background: var(--surface);
    box-shadow: var(--shadow-md);
    color: var(--secondary);
  }
  input {
    flex: 1;
    min-width: 0;
    height: 40px;
    border: 0;
    outline: none;
    background: transparent;
    font-size: 19px;
    color: var(--ink);
  }
  input::placeholder {
    color: var(--muted);
  }
  kbd {
    font-size: 12px;
    padding: 4px 8px;
    border-radius: 6px;
    background: var(--field);
    color: var(--secondary);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 14px;
  }
  .results {
    margin-top: 14px;
    padding: 8px;
    border-radius: 18px;
    display: flex;
    flex-direction: column;
  }
  .hit {
    display: flex;
    align-items: center;
    gap: 14px;
    min-height: 52px;
    padding: 6px 14px;
    border: 0;
    border-radius: 12px;
    background: transparent;
    text-align: left;
  }
  .hit.at {
    background: var(--field);
  }
  .kind {
    width: 72px;
    flex: none;
    font-size: 12px;
    color: var(--muted);
  }
  .what {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .label {
    font-weight: 600;
  }
  .hint {
    font-size: 13px;
    color: var(--secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .key {
    font-size: 12px;
    color: var(--muted);
  }
  .none {
    margin: 0;
    padding: 14px;
    color: var(--secondary);
  }
</style>
