<script lang="ts">
  // Adding an account, or changing an existing one's servers (Settings and
  // the first-run Welcome). Servers are guessed from the address unless typed
  // in; Gmail's are known, so they're behind "Edit settings by hand".
  // ponytail: guessed as imap./smtp.<domain>; ISPDB/SRV autodiscovery later.
  import { act } from '../lib/app.svelte.ts';
  import { api } from '../lib/api.ts';
  import type { Account } from '../lib/gen/index';

  let {
    edit = null,
    title = 'Add an account',
    oncancel = () => {},
    ondone = () => {},
  }: {
    /** The account whose servers change (else a new one). */
    edit?: Account | null;
    title?: string;
    oncancel?: () => void;
    /** After a login that worked (and the first sync started). */
    ondone?: () => void;
  } = $props();

  let email = $state('');
  let password = $state('');
  let adding = $state(false);
  let manual = $state(false);
  let imapHost = $state('');
  let imapPort = $state(993);
  let smtpHost = $state('');
  let smtpPort = $state(465);
  let username = $state('');

  // Editing fills the form with the account's current servers.
  $effect(() => {
    if (!edit) return;
    email = edit.address;
    [imapHost, imapPort, smtpHost, smtpPort, username] = [edit.host, edit.port, edit.smtp_host, edit.smtp_port, edit.username];
    manual = true;
    password = '';
    queueMicrotask(() => document.getElementById('imap-host')?.focus());
  });

  let domain = $derived.by(() => {
    const at = email.trim().lastIndexOf('@');
    const d = at > 0 ? email.trim().slice(at + 1).toLowerCase() : '';
    return d.includes('.') && !d.endsWith('.') ? d : '';
  });
  let gmail = $derived(domain === 'gmail.com' || domain === 'googlemail.com');
  let servers = $derived(manual || !gmail);

  function reset() {
    manual = false;
    [email, password, imapHost, imapPort, smtpHost, smtpPort, username] = ['', '', '', 993, '', 465, ''];
  }

  async function connect() {
    adding = true;
    const address = email.trim();
    const ok = await act(`Connecting to ${address}…`, () =>
      api.accountAdd({
        name: edit?.name ?? address,
        address,
        host: servers && imapHost.trim() ? imapHost.trim() : null,
        port: servers ? imapPort : null,
        username: servers && username.trim() ? username.trim() : null,
        smtp_host: servers && smtpHost.trim() ? smtpHost.trim() : null,
        smtp_port: servers ? smtpPort : null,
        // Gmail shows app passwords in groups of four.
        password: gmail ? password.replace(/\s+/g, '') : password,
      }),
    );
    adding = false;
    if (ok) {
      reset();
      void act('Syncing…', () => api.sync());
      ondone();
    }
  }
</script>

<section class="card add" aria-labelledby="h-add">
  <h2 id="h-add">{edit ? `Change servers for ${edit.label ?? edit.name}` : title}</h2>
  <label for="addr">Email address</label>
  <input id="addr" type="email" placeholder="name@example.com" bind:value={email} autocomplete="email" disabled={edit !== null} />
  {#if !edit}<span class="fine">Type the servers below, or leave them empty and Hysj guesses from the address.</span>{/if}
  {#if domain}
    {#if gmail}
      <p>Gmail connects with an <strong>app password</strong> — a separate password just for Hysj that you can revoke any time.</p>
      <ol>
        <li>Turn on 2-Step Verification for your Google account.</li>
        <li>Open <button class="inline-link" onclick={() => api.openLink('https://myaccount.google.com/apppasswords')}>App passwords</button>, name it “Hysj” and press Create.</li>
        <li>Paste the 16-character password below.</li>
      </ol>
    {/if}
    {#if servers}
      <div class="grid">
        <label for="imap-host">IMAP server</label>
        <input id="imap-host" class="mono" placeholder="imap.{domain}" bind:value={imapHost} autocomplete="off" spellcheck="false" />
        <label for="imap-port">Port</label>
        <input id="imap-port" class="mono port" type="number" min="1" max="65535" bind:value={imapPort} />
        <label for="smtp-host">SMTP server</label>
        <input id="smtp-host" class="mono" placeholder="smtp.{domain}" bind:value={smtpHost} autocomplete="off" spellcheck="false" />
        <label for="smtp-port">Port</label>
        <input id="smtp-port" class="mono port" type="number" min="1" max="65535" bind:value={smtpPort} />
      </div>
      <span class="fine">IMAP uses TLS (usually 993). SMTP 465 uses TLS from the start; 587 upgrades with STARTTLS.</span>
      <label for="user">Username</label>
      <input id="user" placeholder={email.trim()} bind:value={username} autocomplete="off" spellcheck="false" />
    {/if}
    <label for="pw">{gmail ? 'App password' : 'Password'}</label>
    <input id="pw" type="password" class:mono={gmail} placeholder={gmail ? 'xxxx xxxx xxxx xxxx' : ''} bind:value={password} />
    <span class="fine">Kept in your system keychain. Hysj checks it logs in before saving anything.</span>
    <div class="row">
      <button class="btn primary" disabled={adding || !password.trim()} onclick={connect}>{adding ? 'Connecting…' : edit ? 'Save' : 'Connect'}</button>
      {#if edit}
        <button class="btn ghost" onclick={() => (reset(), oncancel())}>Cancel</button>
      {:else if gmail}
        <button class="btn ghost" onclick={() => (manual = !manual)}>{manual ? 'Use Gmail’s settings' : 'Edit settings by hand'}</button>
      {/if}
    </div>
  {/if}
</section>

<style>
  .add {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 24px;
    border-radius: 22px;
  }
  h2 {
    margin: 0 0 4px;
    font-size: 20px;
    font-weight: 600;
  }
  label {
    font-weight: 500;
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
  .fine {
    font-size: 13px;
    color: var(--muted);
  }
  p,
  ol {
    margin: 0;
    line-height: 1.6;
    color: var(--body);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .grid {
    display: grid;
    grid-template-columns: auto 1fr auto 96px;
    align-items: center;
    gap: 10px 12px;
  }
  .port {
    width: 96px;
  }
  .inline-link {
    padding: 2px;
    border: 0;
    background: none;
    font-size: inherit;
    text-decoration: underline;
  }
</style>
