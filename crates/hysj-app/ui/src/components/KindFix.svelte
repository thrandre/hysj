<script lang="ts">
  // "Not a booking" / "Not an order": the reader says what the mail really
  // is. The card goes, and Hysj's classifier learns from it (kinds.rs).
  import { act } from '../lib/app.svelte.ts';
  import { api } from '../lib/api.ts';

  let { record, is }: { record: string; is: 'booking' | 'order' } = $props();
  let open = $state(false);
  const choices: [string, string][] = (
    [
      ['booking', 'A booking'],
      ['order', 'An order'],
      ['subscription', 'A subscription'],
      ['offer', 'An ad or offer'],
      ['other', 'Something else'],
    ] as [string, string][]
  ).filter(([k]) => k !== is && !(is === 'order' && k === 'subscription'));
</script>

{#if open}
  <span class="fix" role="group" aria-label="What is it?">
    <span class="q">It’s</span>
    {#each choices as [kind, label] (kind)}
      <button class="btn small" onclick={() => act('Learning from that…', () => api.kindCorrect(record, kind))}>{label.toLowerCase()}</button>
    {/each}
    <button class="btn small ghost" onclick={() => (open = false)}>Cancel</button>
  </span>
{:else}
  <button class="btn small ghost" onclick={() => (open = true)}>{is === 'booking' ? 'Not a booking' : 'Not an order'}</button>
{/if}

<style>
  .fix {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .q {
    font-size: 13px;
    color: var(--muted);
  }
</style>
