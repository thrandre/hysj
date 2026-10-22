<script lang="ts">
  // Settings: accounts, when the digest comes, the on-device AI, appearance
  // and about.
  import { act, app, accountSlot, setTheme } from '../lib/app.svelte.ts';
  import { api } from '../lib/api.ts';
  import type { Account, AiSettings } from '../lib/gen/index';
  import AccountForm from '../components/AccountForm.svelte';
  import DigestHours from '../components/DigestHours.svelte';

  let tab = $state<'accounts' | 'digest' | 'ai' | 'appearance' | 'about'>('accounts');
  /** The account being renamed, and the name typed so far. */
  let renaming = $state<string | null>(null);
  let newLabel = $state('');
  /** The account whose servers are being changed. */
  let editing = $state<Account | null>(null);
  let ai = $state<AiSettings | null>(null);
  let version = $state('');

  $effect(() => {
    if (tab === 'ai' && !ai) void api.aiSettings().then((s) => (ai = s));
    if (tab === 'about' && !version) void api.appVersion().then((v) => (version = v));
  });

  async function rename(a: Account) {
    const label = newLabel.trim();
    if ((await act(null, () => api.accountLabel(a.name, label || null))) !== undefined) renaming = null;
  }

  async function setAi(enabled: boolean) {
    const s = await act(null, () => api.aiSettings(enabled));
    if (s) ai = s;
  }

  const WORDS = ['No', 'One', 'Two', 'Three', 'Four', 'Five'];
  let n = $derived(app.data!.accounts.length);
  let headline = $derived(n === 0 ? 'No accounts yet.' : `${WORDS[n] ?? n} account${n === 1 ? '' : 's'}, one Desk.`);
</script>

<div class="column">
  <p class="meta">Settings</p>
  <div class="tabs" role="tablist">
    {#each [['accounts', 'Accounts'], ['digest', 'Digest'], ['ai', 'On-device AI'], ['appearance', 'Appearance'], ['about', 'About']] as [id, label] (id)}
      <button role="tab" aria-selected={tab === id} onclick={() => (tab = id as typeof tab)}>{label}</button>
    {/each}
  </div>

  {#if tab === 'accounts'}
    <h1 class="h1">{headline}</h1>
    <p class="lede">Someone you know in one account is known everywhere — they skip the Screener in all of them.</p>
    {#if n}
      <div class="card accounts">
        {#each app.data!.accounts as a (a.name)}
          <div class="account">
            <span class="dot" style:background="var(--acct-{accountSlot(a.name)})"></span>
            <div>
              {#if renaming === a.name}
                <form class="rename" onsubmit={(e) => (e.preventDefault(), rename(a))}>
                  <label for="label-{a.name}" class="sr-only">Name for {a.address}</label>
                  <!-- svelte-ignore a11y_autofocus -->
                  <input id="label-{a.name}" bind:value={newLabel} placeholder={a.name} maxlength="40" autofocus />
                  <button class="btn small primary" type="submit">Save</button>
                  <button class="btn small ghost" type="button" onclick={() => (renaming = null)}>Cancel</button>
                </form>
              {:else}
                <span><strong>{a.label ?? a.name}</strong> <span class="addr">{a.address}</span></span>
              {/if}
              <span class="how">
                {a.host === 'imap.gmail.com' ? 'Gmail · IMAP with app password · labels kept as tags' : `IMAP ${a.host} · SMTP ${a.smtp_host} · password in keychain`}
              </span>
            </div>
            {#if renaming !== a.name}
              <button class="btn small ghost" onclick={() => ((renaming = a.name), (newLabel = a.label ?? ''))}>Rename</button>
            {/if}
            <button class="btn small ghost" onclick={() => (editing = a)}>Edit</button>
          </div>
        {/each}
      </div>
    {/if}
    {#key editing?.name}
      <AccountForm edit={editing} oncancel={() => (editing = null)} ondone={() => (editing = null)} />
    {/key}
  {:else if tab === 'digest'}
    <h1 class="h1">When should the rest arrive?</h1>
    <p class="lede">People reach you right away. Newsletters, notifications and receipts wait for your digest. Anything that can’t wait still comes through.</p>
    <DigestHours />
  {:else if tab === 'ai'}
    <h1 class="h1">Gists and replies are written on this Mac.</h1>
    <p class="lede">
      A small language model (Gemma 3 4B) reads only what reaches you — people’s mail and the latest digest — and writes one-line gists, suggested replies and
      the digest’s headline. Nothing leaves your computer. When it isn’t sure, you see the real subject line.
    </p>
    <div class="card facts">
      <p>Downloaded once (about 4 GB) into Hysj’s data folder.</p>
      <p>Loaded when there is something to write, unloaded after 10 idle minutes.</p>
      <p>Codes, blocked senders and your own mail are never given to it.</p>
    </div>
    {#if ai}
      <label class="switch">
        <input type="checkbox" checked={ai.enabled} onchange={(e) => setAi(e.currentTarget.checked)} />
        <span>Use the on-device AI</span>
      </label>
      {#if !ai.enabled}
        <p class="fine">Sorting still works with rules alone. You’ll see subject lines instead of summaries.</p>
      {/if}
    {/if}
  {:else if tab === 'about'}
    <h1 class="h1">Hysj {version}</h1>
    <p class="lede">Built from this computer’s copy of Hysj. It doesn’t update itself yet: get the latest by building again (<span class="mono">cargo run</span>).</p>
    <div class="versions">
      {#each [['Hysj app', ''], ['Background service', 'updates with the app'], ['Command line (hysj)', 'updates with the app']] as [label, note] (label)}
        <div class="version"><span>{label}</span><span class="fine">{note}</span><span class="mono">{version}</span></div>
      {/each}
      <div class="version"><span>On-device AI</span><span class="fine">downloaded separately</span><span class="mono">Gemma 3 4B</span></div>
    </div>
  {:else}
    <h1 class="h1">Light, dark, or follow the system.</h1>
    <div class="segmented" role="group" aria-label="Appearance">
      {#each [['system', 'System'], ['light', 'Light'], ['dark', 'Dark']] as [v, label] (v)}
        <button aria-pressed={app.theme === v} onclick={() => setTheme(v as 'system' | 'light' | 'dark')}>{label}</button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .rename {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .rename input {
    min-width: 0;
    flex: 1 1 160px;
  }
  .column {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .meta {
    margin: 0;
  }
  .tabs {
    display: flex;
    gap: 4px;
    margin-bottom: 8px;
  }
  .tabs button {
    min-height: 36px;
    padding: 0 14px;
    border: 0;
    border-radius: 999px;
    background: transparent;
    font-size: 13px;
  }
  .tabs button[aria-selected='true'] {
    background: var(--surface);
  }
  .accounts {
    padding: 6px;
    border-radius: 22px;
  }
  .account {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 16px;
  }
  .account div {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .dot {
    flex: none;
    width: 12px;
    height: 12px;
    border-radius: 50%;
  }
  .addr {
    color: var(--secondary);
  }
  .how,
  .fine {
    font-size: 13px;
    color: var(--muted);
  }
  input {
    height: 48px;
    padding: 0 14px;
    border: 0;
    border-radius: 12px;
    background: var(--field);
    font-size: 16px;
    user-select: text;
    -webkit-user-select: text;
  }
  .facts {
    padding: 22px;
  }
  .switch {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 44px;
    cursor: pointer;
  }
  .switch input {
    width: 20px;
    height: 20px;
    accent-color: var(--primary);
  }
  .versions {
    display: flex;
    flex-direction: column;
  }
  .version {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 16px;
    min-height: 52px;
    border-bottom: 1px solid var(--raised);
  }
  .version span:first-child {
    flex: 1 1 200px;
  }
  .facts p {
    margin: 4px 0;
    color: var(--body);
  }
  .segmented {
    display: flex;
    align-self: flex-start;
    gap: 2px;
    padding: 3px;
    border-radius: 999px;
    background: var(--raised);
  }
  .segmented button {
    min-height: 36px;
    padding: 0 16px;
    border: 0;
    border-radius: 999px;
    background: transparent;
  }
  .segmented button[aria-pressed='true'] {
    background: var(--segment);
    box-shadow: 0 1px 3px rgba(17, 17, 17, 0.12);
  }
</style>
