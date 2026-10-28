<script lang="ts">
  import { app, perform, toggle } from '../lib/app.svelte.ts';
  import { api } from '../lib/api.ts';
  import { hits, isQuestion, matches, suggestions } from '../lib/hits.ts';
  import { shortMonth } from '../lib/findit.ts';
  import type { Asked } from '../lib/types.ts';
  import { firstName, subject } from '../lib/words.ts';
  import Inline from './Inline.svelte';

  let query = $state('');
  let at = $state(0);
  /** Ask your mail: the question asked, and its answer once it's in. */
  let asked = $state<{ q: string; a: Asked | null } | null>(null);
  let input: HTMLInputElement | undefined = $state();

  const now = () => Date.now() / 1000;
  // What's loaded first; the last line always searches all mail on the server.
  let results = $derived(
    query.trim() && app.data
      ? [
          ...(isQuestion(query) && asked?.q !== query.trim() ? [{ kind: 'Ask', label: `Ask your mail: “${query.trim()}”`, hint: 'answered on this computer, only from your own mail', go: { ask: query.trim() } }] : []),
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
    if ('ask' in h.go) return void ask(h.go.ask);
    query = '';
    perform(h.go);
  }

  $effect(() => {
    if (asked && asked.q !== query.trim()) asked = null;
  });
  // A question ending in "?" is asked once typing stops; others on Enter.
  $effect(() => {
    const q = query.trim();
    if (!q.endsWith('?') || asked?.q === q) return;
    const t = setTimeout(() => ask(q), 700);
    return () => clearTimeout(t);
  });

  async function ask(q: string) {
    asked = { q, a: null };
    let a: Asked;
    try {
      a = await api.ask(q);
    } catch (e) {
      a = { answer: null, note: `Couldn’t ask: ${e}` };
    }
    if (asked?.q === q) asked = { q, a };
  }

  const day = (ts: bigint | number | null) => {
    if (ts == null) return '';
    const d = new Date(Number(ts) * 1000);
    return `${d.getDate()} ${shortMonth(d)}`;
  };

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
  {#if asked}
    <div class="answer card" role="region" aria-label="Answer" aria-live="polite" aria-busy={!asked.a}>
      {#if !asked.a}
        <span class="note">Answer · looking through your mail on this computer…</span>
        <span class="thinking" aria-hidden="true"></span>
      {:else if asked.a.answer}
        {@const a = asked.a.answer}
        <span class="note">Answer · from {a.sources.length === 1 ? '1 email' : `${a.sources.length} emails`} · worked out on this computer</span>
        <p class="text">{a.text}</p>
        <div class="sources">
          {#each a.sources as s (s.id)}
            <button class="source" aria-expanded={app.open === s.id} onclick={() => toggle(s.id)}
              ><strong>{s.from}</strong><span>{subject(s.subject)} · {day(s.ts)}</span></button
            >
          {/each}
        </div>
        {#each a.sources as s (s.id)}
          {#if app.open === s.id}<Inline id={s.id} />{/if}
        {/each}
        <span class="note">{a.caveat}</span>
      {:else}
        <p class="text none-found">{asked.a.note}</p>
        <span class="note">Hysj only answers from mail on this computer, and never guesses.</span>
      {/if}
    </div>
  {/if}
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
  .answer {
    margin-top: 14px;
    padding: 20px 22px;
    border-radius: 18px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .answer .note {
    font-size: 12px;
    color: var(--muted);
  }
  .text {
    margin: 0;
    font-size: 19px;
    line-height: 1.5;
    color: var(--ink);
  }
  .none-found {
    font-size: 17px;
    color: var(--body);
  }
  .sources {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .source {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 32px;
    padding: 0 12px;
    border: 0;
    border-radius: 999px;
    background: var(--field);
    font-size: 13px;
  }
  .source span {
    color: var(--secondary);
  }
  .thinking {
    width: 120px;
    height: 6px;
    border-radius: 3px;
    background: linear-gradient(90deg, var(--field) 0%, var(--raised) 50%, var(--field) 100%);
    background-size: 200% 100%;
    animation: shimmer 1.4s linear infinite;
  }
  @keyframes shimmer {
    to {
      background-position: -200% 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .thinking {
      animation: none;
    }
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
