<script lang="ts">
  // Replying to a message (Desk panel, Focus): suggested replies, From with
  // its hint, the reply and Send (⌘↩).
  import { untrack } from 'svelte';
  import { act, app } from '../lib/app.svelte.ts';
  import { api } from '../lib/api.ts';
  import type { MessageSummary } from '../lib/types.ts';
  import { firstName, replyHint } from '../lib/words.ts';

  let { m, onsent = () => {}, autofocus = false }: { m: MessageSummary; onsent?: () => void; autofocus?: boolean } = $props();

  // One draft per message, kept while you look elsewhere (this box is keyed by message).
  untrack(() => {
    if (!app.drafts[m.id]) app.drafts[m.id] = { from: m.arrived_on, text: '', sending: false };
  });
  let draft = $derived(app.drafts[m.id] ?? { from: m.arrived_on, text: '', sending: false });
  let hint = $derived(replyHint(draft.from, m.arrived_on, app.data?.identities ?? [], m.from));
  let field: HTMLTextAreaElement | undefined = $state();
  $effect(() => {
    if (autofocus) field?.focus();
  });

  async function send() {
    if (!draft.text.trim() || draft.sending) return;
    draft.sending = true;
    const ok = await act('Sending…', () => api.send({ from: draft.from, to: [], cc: [], subject: '', body: draft.text, reply_to: m.id }));
    draft.sending = false;
    if (ok) {
      delete app.drafts[m.id];
      onsent();
    }
  }
</script>

<div class="box">
  {#if m.replies.length}
    <div class="chips">
      {#each m.replies as r (r)}
        <button class="chip" onclick={() => (draft.text = r)}>{r}</button>
      {/each}
    </div>
  {/if}
  <div class="from">
    <label for="from-{m.id}">From</label>
    <select id="from-{m.id}" bind:value={draft.from}>
      {#each app.data?.identities ?? [] as i (i.address)}
        <option value={i.address}>{i.address}</option>
      {/each}
    </select>
    <span class:warn={hint[1]}>{hint[0]}</span>
  </div>
  <div class="reply">
    <label class="sr-only" for="reply-{m.id}">Reply to {m.from}</label>
    <textarea
      id="reply-{m.id}"
      bind:this={field}
      rows="1"
      placeholder="Reply to {firstName(m.from)}…"
      bind:value={draft.text}
      onkeydown={(e) => {
        if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) send();
      }}
    ></textarea>
    <button class="btn primary" disabled={!draft.text.trim() || draft.sending} onclick={send} title="Send (⌘↩)">
      {draft.sending ? 'Sending…' : 'Send'}
    </button>
  </div>
</div>

<style>
  .box {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    background: var(--field);
    min-height: 32px;
  }
  .from {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--secondary);
    min-width: 0;
  }
  .from select {
    max-width: 220px;
    height: 30px;
    padding: 0 8px;
    border: 0;
    border-radius: 8px;
    background: var(--field);
    font-size: 13px;
  }
  .from span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted);
  }
  .from span.warn {
    color: var(--warn);
  }
  .reply {
    display: flex;
    align-items: flex-end;
    gap: 8px;
    padding: 6px 6px 6px 14px;
    border-radius: 14px;
    background: var(--field);
  }
  textarea {
    flex: 1;
    min-height: 40px;
    max-height: 200px;
    padding: 10px 0;
    border: 0;
    outline: none;
    resize: none;
    background: transparent;
    field-sizing: content;
  }
  .reply .btn {
    min-height: 40px;
  }
</style>
