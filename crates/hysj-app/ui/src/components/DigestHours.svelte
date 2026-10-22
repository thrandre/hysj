<script lang="ts">
  // When the digest arrives: whole hours, one to five a day (Welcome and
  // Settings). Saved as soon as it changes.
  import { act, app } from '../lib/app.svelte.ts';
  import { api } from '../lib/api.ts';

  const CHOICES = [7, 8, 12, 16, 19];
  let hours = $derived(app.data?.digestHours ?? [8, 16]);
  const hh = (h: number) => `${String(h).padStart(2, '0')}:00`;
  let summary = $derived(
    hours.length === 1 ? `One digest a day, at ${hh(hours[0])}.` : `${hours.length} digests a day: ${hours.map(hh).join(', ')}.`,
  );

  function toggle(h: number) {
    const next = hours.includes(h) ? hours.filter((x) => x !== h) : [...hours, h].sort((a, b) => a - b);
    // At least one: a digest that never comes would hold mail forever.
    if (next.length) void act(null, () => api.digestHours(next));
  }
</script>

<div class="hours" role="group" aria-label="Digest times">
  {#each CHOICES as h (h)}
    <button class="mono" aria-pressed={hours.includes(h)} onclick={() => toggle(h)}>{hh(h)}</button>
  {/each}
</div>
<span class="summary">{summary}</span>

<style>
  .hours {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .hours button {
    height: 48px;
    padding: 0 18px;
    border: 0;
    border-radius: 14px;
    background: var(--surface);
    box-shadow: 0 4px 12px rgba(17, 17, 17, 0.05);
    font-size: 15px;
    color: var(--ink);
    cursor: pointer;
  }
  .hours button[aria-pressed='true'] {
    background: var(--primary);
    color: var(--on-primary);
    box-shadow: none;
  }
  .summary {
    color: var(--secondary);
  }
</style>
