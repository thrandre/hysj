<script lang="ts">
  // A message opened in place (Digest rows, Sent, Done, receipts).
  import { app } from '../lib/app.svelte.ts';
  import MailFrame from './MailFrame.svelte';

  let { id }: { id: string } = $props();
  let view = $derived(app.views[id]);
  let folded = $state(0);
  let showFolded = $state(false);
</script>

<div class="inline">
  {#if !view}
    <p class="loading">Loading…</p>
  {:else}
    <MailFrame {view} {showFolded} onfolded={(n) => (folded = n)} onmailto={(to) => (app.compose = { to })} />
    {#if folded > 0}
      <button class="fold" onclick={() => (showFolded = !showFolded)}>{showFolded ? 'Hide quoted text' : 'Quoted text hidden — show'}</button>
    {/if}
  {/if}
</div>

<style>
  .inline {
    margin: 8px 0 16px;
    user-select: text;
    -webkit-user-select: text;
  }
  .loading {
    color: var(--muted);
  }
  .fold {
    min-height: 32px;
    border: 0;
    background: none;
    color: var(--muted);
    font-size: 12px;
  }
</style>
