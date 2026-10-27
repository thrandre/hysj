<script lang="ts">
  import { act, markDone, accountLabel, accountSlot, app, holdSender, toggle } from '../lib/app.svelte.ts';
  import { api } from '../lib/api.ts';
  import type { MessageSummary } from '../lib/types.ts';
  import { age, fileSize, firstName, reasonText } from '../lib/words.ts';
  import MailFrame from './MailFrame.svelte';
  import ReplyBox from './ReplyBox.svelte';

  let { m }: { m: MessageSummary } = $props();

  let view = $derived(app.views[m.id]);
  let folded = $state(0);
  let showFolded = $state(false);
  let slot = $derived(accountSlot(m.account));
</script>

<aside class="panel" aria-label="Conversation with {m.from}">
  <header>
    <div class="who">
      <span class="name">{m.from}</span>
      <span class="sub"><span class="dot" style:background="var(--acct-{slot})"></span>{accountLabel(m.account)}{m.ts ? ` · ${age(m.ts, Date.now() / 1000)}` : ''}</span>
    </div>
    <button class="btn small ghost" onclick={() => { void markDone([m.id]); app.open = null; }} title="Clear from the Desk">Done</button>
    <button class="close" aria-label="Close" onclick={() => toggle(m.id)}>×</button>
  </header>
  <div class="why">
    <span class="because"><strong>Why here:</strong> {reasonText(m.reason)}</span>
    <button class="chip" onclick={() => holdSender(m, `mail like this from ${firstName(m.from)} now goes to the digest.`)}>Not a person</button>
    <button class="chip" onclick={() => holdSender(m, `${firstName(m.from)}’s mail now waits for the digest.`)}>Hold these</button>
  </div>

  <div class="body">
    {#if !view}
      <p class="loading">Loading…</p>
    {:else}
      <MailFrame view={view} {showFolded} onfolded={(n) => (folded = n)} onmailto={(to) => (app.compose = { to })} />
      {#if folded > 0}
        <button class="fold" onclick={() => (showFolded = !showFolded)}>
          {showFolded ? 'Hide quoted text and signature' : 'Quoted text and signature hidden — show'}
        </button>
      {/if}
      {#if view.attachments.length}
        <ul class="files" aria-label="Attachments">
          {#each view.attachments as a (a.index)}
            <li>
              <span class="file"><span class="fname">{a.name}</span><span class="fsize mono">{fileSize(a.size)}</span></span>
              <button class="btn small" onclick={() => act('Opening…', () => api.attachmentSave(m.id, a.index, true))}>Open</button>
              <button class="btn small ghost" onclick={() => act('Saving…', () => api.attachmentSave(m.id, a.index, false))}>Save</button>
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
  </div>

  <footer>
    <ReplyBox {m} onsent={() => (app.open = null)} />
  </footer>
</aside>

<style>
  .why {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 8px;
    margin: 0 18px 8px 22px;
    padding: 10px 12px;
    border-radius: 12px;
    background: var(--ground);
    font-size: 13px;
    color: var(--secondary);
  }
  .because {
    flex: 1 1 200px;
  }
  .because strong {
    font-weight: 600;
    color: var(--ink);
  }
  .chip {
    min-height: 30px;
    padding: 0 10px;
    border: 0;
    border-radius: 999px;
    background: var(--surface);
    font: inherit;
    font-size: 13px;
    color: var(--ink);
    cursor: pointer;
  }
  .panel {
    position: fixed;
    top: 76px;
    right: 24px;
    bottom: 104px;
    z-index: 5;
    width: min(460px, calc(100vw - 48px));
    display: flex;
    flex-direction: column;
    border-radius: 22px;
    background: var(--surface);
    box-shadow: var(--shadow-lg);
    overflow: hidden;
    user-select: text;
    -webkit-user-select: text;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 18px 18px 12px 22px;
  }
  .who {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .name {
    font-weight: 600;
    font-size: 17px;
  }
  .sub {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    color: var(--secondary);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }
  .close {
    width: 40px;
    height: 40px;
    border: 0;
    border-radius: 12px;
    background: var(--field);
    color: var(--secondary);
    font-size: 18px;
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 8px 22px 16px;
  }
  .loading {
    color: var(--muted);
    font-size: 13px;
  }
  .fold {
    display: block;
    margin: 4px auto 0;
    min-height: 32px;
    border: 0;
    background: none;
    color: var(--muted);
    font-size: 12px;
  }
  .files {
    list-style: none;
    margin: 12px 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .files li {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 6px 6px 12px;
    border-radius: 12px;
    background: var(--field);
  }
  .file {
    flex: 1;
    min-width: 0;
    display: flex;
    gap: 10px;
    align-items: baseline;
  }
  .fname {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .fsize {
    font-size: 12px;
    color: var(--muted);
  }
  footer {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 18px 18px;
    border-top: 1px solid var(--line);
  }
</style>
