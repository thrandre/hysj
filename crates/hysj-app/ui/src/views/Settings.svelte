<script lang="ts">
  // Settings: accounts, when the digest comes, the on-device AI, appearance
  // and about.
  import { act, app, accountSlot, copy, setTheme } from '../lib/app.svelte.ts';
  import { api } from '../lib/api.ts';
  import type { Account, AiSettings } from '../lib/gen/index';
  import AccountForm from '../components/AccountForm.svelte';
  import DigestHours from '../components/DigestHours.svelte';


  /** The account being renamed, and the name typed so far. */
  let renaming = $state<string | null>(null);
  let newLabel = $state('');
  /** The account whose servers are being changed. */
  let editing = $state<Account | null>(null);
  let ai = $state<AiSettings | null>(null);
  let version = $state('');

  $effect(() => {
    if (app.settingsTab === 'ai' && !ai) void api.aiSettings().then((s) => (ai = s));
    if (app.settingsTab === 'about' && !version) void api.appVersion().then((v) => (version = v));
  });

  async function rename(a: Account) {
    const label = newLabel.trim();
    if ((await act(null, () => api.accountLabel(a.name, label || null))) !== undefined) renaming = null;
  }

  /** For bug reports: versions, the Mac and counts — no addresses, names or mail. */
  async function diagnostics() {
    const d = app.data!;
    const ai = await api.aiSettings().catch(() => null);
    const kinds = d.accounts.map((a) => (a.host === 'imap.gmail.com' ? 'Gmail' : 'IMAP')).join(', ');
    const lines = [
      `Hysj ${await api.appVersion()} (${app.update?.can_update ? 'release' : 'built locally'})`,
      await api.systemInfo(),
      `Accounts: ${d.accounts.length} (${kinds || 'none'})`,
      `Mail: ${d.people.length} people · ${d.digest.length} digest · ${d.screener.length} screener · ${d.done.length} done`,
      `Found: ${d.codes.length} codes · ${d.bookings.length} bookings · ${d.orders.length} orders`,
      `Digest at: ${d.digestHours.map((h) => `${String(h).padStart(2, '0')}:00`).join(', ')}`,
      ai ? `AI: ${ai.enabled ? ai.model : 'off'}${ai.enabled && !ai.on_battery ? ' · pauses on battery' : ''}` : 'AI: unknown',
    ];
    await copy(lines.join('\n'), 'diagnostics');
  }

  async function setAi(change: Partial<AiSettings>) {
    const s = await act(null, () => api.aiSettings({ ...ai!, ...change }));
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
      <button role="tab" aria-selected={app.settingsTab === id} onclick={() => (app.settingsTab = id as typeof app.settingsTab)}>{label}</button>
    {/each}
  </div>

  {#if app.settingsTab === 'accounts'}
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
  {:else if app.settingsTab === 'digest'}
    <h1 class="h1">When should the rest arrive?</h1>
    <p class="lede">People reach you right away. Newsletters, notifications and receipts wait for your digest. Anything that can’t wait still comes through.</p>
    <DigestHours />
  {:else if app.settingsTab === 'ai'}
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
        <input type="checkbox" checked={ai.enabled} onchange={(e) => setAi({ enabled: e.currentTarget.checked })} />
        <span>Use the on-device AI</span>
      </label>
      {#if !ai.enabled}
        <p class="fine">Sorting still works with rules alone. You’ll see subject lines instead of summaries.</p>
      {:else}
        <label class="switch">
          <input type="checkbox" checked={!ai.on_battery} onchange={(e) => setAi({ on_battery: !e.currentTarget.checked })} />
          <span>Pause on battery</span>
        </label>
        <p class="fine">
          {ai.waiting_for_power
            ? 'Waiting for the charger: summaries catch up when you plug in.'
            : 'Writing summaries works the processor hard; on battery it waits until you plug in.'}
        </p>
      {/if}
    {/if}
  {:else if app.settingsTab === 'about'}
    {@const u = app.update}
    <h1 class="h1">{u?.ready ? `Hysj ${u.ready} is ready to install.` : `You’re on Hysj ${version}.`}</h1>
    <p class="lede">
      {#if !u?.can_update}
        This copy was built on this computer, so it doesn’t update itself: build again (<span class="mono">cargo run</span>) for the latest.
      {:else if u.ready}
        It’s downloaded and verified. Restart whenever it suits you — or it installs the next time you quit.
      {:else}
        Updates download quietly and install when you restart or quit.
      {/if}
    </p>
    {#if u?.ready}
      <section class="card news" aria-labelledby="h-new">
        <h2 id="h-new">What’s new in {u.ready}</h2>
        {#if u.notes}<p>{u.notes}</p>{/if}
        <div class="row">
          <button class="btn primary" onclick={() => api.updateRestart()}>Restart to update</button>
        </div>
      </section>
    {/if}
    <div class="versions">
      {#each [['Hysj app', ''], ['Background service', 'updates with the app'], ['Command line (hysj)', 'updates with the app']] as [label, note] (label)}
        <div class="version"><span>{label}</span><span class="fine">{note}</span><span class="mono">{version}</span></div>
      {/each}
      <div class="version"><span>On-device AI</span><span class="fine">downloaded separately</span><span class="mono">Gemma 3 4B</span></div>
    </div>
    <div class="row">
      <button class="btn" onclick={diagnostics}>{app.copied === 'diagnostics' ? 'Copied' : 'Copy diagnostics'}</button>
      <span class="fine">For a bug report: versions, your Mac and counts — no addresses, names or mail.</span>
    </div>
    {#if u?.can_update}
      <section class="updates" aria-labelledby="h-upd">
        <h2 id="h-upd">Updates</h2>
        <label class="switch">
          <input type="checkbox" checked={u.auto} onchange={(e) => api.updateAuto(e.currentTarget.checked).then((s) => (app.update = s))} />
          <span>Check for updates automatically</span>
        </label>
        <p class="fine">Hysj asks GitHub a few times a day whether a new version exists. GitHub sees your IP address and version — nothing about your mail.</p>
        <div class="row">
          <button class="btn" onclick={() => act('Checking for updates…', () => api.updateCheck()).then((s) => s && (app.update = s))}>Check now</button>
          {#if u.checked}<span class="fine">Last checked {new Date(u.checked * 1000).toLocaleString('en-GB', { weekday: 'long', hour: '2-digit', minute: '2-digit' })}</span>{/if}
        </div>
      </section>
    {/if}
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
  .news {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 24px;
  }
  .news h2,
  .updates h2 {
    margin: 0;
    font-size: 20px;
    font-weight: 600;
  }
  .news p {
    margin: 0;
    line-height: 1.55;
    color: var(--body);
  }
  .updates {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 16px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
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
