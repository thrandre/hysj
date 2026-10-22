<script lang="ts">
  // A new email: To with suggestions from people you've written to, From picked
  // from history (overriding it is flagged), Subject, message. Esc closes.
  import { accountLabel, act, app } from '../lib/app.svelte.ts';
  import { api } from '../lib/api.ts';
  import type { IdentitySuggestion } from '../lib/types.ts';

  let to = $state(app.compose?.to ?? '');
  let from = $state(app.data?.identities[0]?.address ?? '');
  let touched = $state(false);
  let subject = $state('');
  let body = $state('');
  let sending = $state(false);
  let suggestion = $state<IdentitySuggestion | null>(null);
  let asked = '';

  /** The address in "Name <a@b>" or "a@b", if it looks like one. */
  const address = (t: string) => {
    const last = t.split(',').at(-1)?.trim() ?? '';
    const a = (last.includes('<') ? last.slice(last.lastIndexOf('<') + 1).replace('>', '') : last).trim();
    const [local, domain] = a.split('@');
    return local && domain?.includes('.') ? a.toLowerCase() : null;
  };
  let matches = $derived.by(() => {
    const q = to.trim().toLowerCase();
    if (!q || address(to)) return [];
    return (app.data?.contacts ?? []).filter((k) => k.address.includes(q) || k.name?.toLowerCase().includes(q)).slice(0, 5);
  });
  $effect(() => {
    const a = address(to);
    if (a && a !== asked) {
      asked = a;
      api.identitySuggest(a).then((s) => {
        suggestion = s;
        if (!touched) from = s.identity;
      });
    }
  });
  let hint = $derived.by((): [string, boolean] => {
    const s = suggestion;
    if (!s) return ['Your default for new email', false];
    if (s.identity.toLowerCase() !== from.toLowerCase()) return [s.usual ? `You usually write to them from ${s.identity}` : 'Changed from your default', true];
    return [s.usual ? `Picked automatically — ${s.reason.toLowerCase()}` : s.reason, false];
  });

  async function send() {
    sending = true;
    const ok = await act('Sending…', () => api.send({ from, to: [to], cc: [], subject, body, reply_to: null }));
    sending = false;
    if (ok) app.compose = null;
  }
</script>

<div class="compose card" role="dialog" aria-label="New email">
  <div class="head">
    <strong>New email</strong>
    <button class="close" aria-label="Close (Esc)" onclick={() => (app.compose = null)}>×</button>
  </div>
  <div class="field">
    <label for="c-to">To</label>
    <!-- svelte-ignore a11y_autofocus -->
    <input id="c-to" bind:value={to} placeholder="Name or address" autocomplete="off" autofocus />
  </div>
  {#each matches as k (k.address)}
    <button class="contact" onclick={() => (to = k.name ? `"${k.name.replace(/"/g, "'")}" <${k.address}>` : k.address)}>
      {k.name ? `${k.name} · ${k.address}` : k.address}
    </button>
  {/each}
  <div class="field">
    <label for="c-from">From</label>
    <select id="c-from" bind:value={from} onchange={() => (touched = true)}>
      {#each app.data?.identities ?? [] as i (i.address)}<option value={i.address}>{i.address} · {accountLabel(i.account)}</option>{/each}
    </select>
  </div>
  <p class="hint" class:warn={hint[1]}>{hint[0]}</p>
  <div class="field">
    <label for="c-subject">Subject</label>
    <input id="c-subject" bind:value={subject} />
  </div>
  <label class="sr-only" for="c-body">Message</label>
  <textarea
    id="c-body"
    rows="8"
    placeholder="Message"
    bind:value={body}
    onkeydown={(e) => {
      if (e.key === 'Enter' && (e.metaKey || e.ctrlKey) && address(to) && body.trim()) send();
    }}
  ></textarea>
  <div class="actions">
    <button class="btn primary" disabled={sending || !address(to) || !body.trim()} onclick={send} title="Send (⌘↩)">{sending ? 'Sending…' : 'Send'}</button>
    <button class="btn ghost" disabled={sending} onclick={() => (app.compose = null)}>Discard</button>
  </div>
</div>

<style>
  .compose {
    position: fixed;
    right: 24px;
    bottom: 96px;
    z-index: 30;
    width: min(520px, calc(100vw - 48px));
    padding: 14px 22px 20px;
    border-radius: 20px;
    box-shadow: var(--shadow-lg);
    display: flex;
    flex-direction: column;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 4px;
    font-size: 17px;
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
  .field {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 44px;
    border-bottom: 1px solid var(--line);
  }
  .field label {
    width: 56px;
    color: var(--muted);
  }
  .field input,
  .field select {
    flex: 1;
    min-width: 0;
    height: 40px;
    border: 0;
    outline: none;
    background: transparent;
  }
  .contact {
    margin-left: 64px;
    min-height: 32px;
    border: 0;
    background: none;
    text-align: left;
    font-size: 13px;
  }
  .hint {
    margin: 4px 0 8px 64px;
    font-size: 12px;
    color: var(--muted);
  }
  .hint.warn {
    color: var(--warn);
  }
  textarea {
    margin-top: 12px;
    border: 0;
    outline: none;
    resize: vertical;
    background: transparent;
    line-height: 1.5;
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-top: 12px;
  }
  input,
  textarea {
    user-select: text;
    -webkit-user-select: text;
  }
</style>
