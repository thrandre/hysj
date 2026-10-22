<script lang="ts">
  // Adding an account, or changing an existing one's servers (Settings and
  // the first-run Welcome). The daemon says who an address belongs to — its
  // servers and how it signs in (provider.rs) — and the form follows: a
  // password, an app password with the provider's steps, "Sign in with …",
  // or (Gmail) how to add it from the command line.
  import { act } from '../lib/app.svelte.ts';
  import { api } from '../lib/api.ts';
  import type { Account, ProviderInfo } from '../lib/gen/index';

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
  /** The daemon's answer, and the domain it's about (never shown for another). */
  let answer = $state<{ domain: string; info: ProviderInfo } | null>(null);

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

  let info = $derived(answer && answer.domain === domain ? answer.info : null);

  // Ask the daemon about the address once typing pauses.
  $effect(() => {
    const address = email.trim();
    const d = domain;
    if (!d) return;
    const t = setTimeout(() => {
      api.accountDiscover(address).then(
        (i) => {
          answer = { domain: d, info: i };
          // The provider's ports, unless editing an account's own.
          if (!edit) [imapPort, smtpPort] = [i.imap_port, i.smtp_port];
        },
        () => (answer = null),
      );
    }, 200);
    return () => clearTimeout(t);
  });

  /** Server fields: always for an unknown domain, on request for a known one. */
  let servers = $derived(manual || (info !== null && !info.known));
  let appPassword = $derived(info?.auth === 'app_password' || (edit !== null && info?.auth === 'command_line'));

  function reset() {
    manual = false;
    answer = null;
    [email, password, imapHost, imapPort, smtpHost, smtpPort, username] = ['', '', '', 993, '', 465, ''];
  }

  async function signIn() {
    adding = true;
    const address = email.trim();
    const ok = await act(`Waiting for you to sign in to ${info?.sign_in_with ?? 'your provider'} in your browser…`, () => api.accountOAuth(address));
    adding = false;
    if (ok) {
      reset();
      void act('Syncing…', () => api.sync());
      ondone();
    }
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
        // App passwords are shown in groups ("abcd efgh …"); never spaces in them.
        password: appPassword ? password.replace(/\s+/g, '') : password,
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
  {#if domain && info}
    {#if info.auth === 'oauth'}
      <p>
        Sign in with {info.sign_in_with} in your browser — Hysj never sees your password, and you can remove its access any time in your
        {info.sign_in_with} account.
      </p>
      {#each info.help_steps as note (note)}<span class="fine">{note}</span>{/each}
      <div class="row">
        <button class="btn primary" disabled={adding} onclick={signIn}>
          {adding ? `Waiting for ${info.sign_in_with}…` : edit ? 'Sign in again' : `Sign in with ${info.sign_in_with}`}
        </button>
        {#if edit}<button class="btn ghost" onclick={() => (reset(), oncancel())}>Cancel</button>{/if}
      </div>
    {:else if info.auth === 'command_line' && !edit}
      <p>{info.help_label}</p>
      <ol>
        {#each info.help_steps as step (step)}<li>{step}</li>{/each}
      </ol>
      <span class="fine">Hysj’s source code and how to build it: github.com/thrandre/hysj.</span>
    {:else}
      {#if !info.known}<span class="fine">Hysj guessed the servers for {info.name}; change them if your provider uses others.</span>{/if}
      {#if appPassword && info.help_url}
        <p>{info.name} connects with an <strong>app password</strong> — a separate password just for Hysj that you can revoke any time.</p>
        <ol>
          {#each info.help_steps as step (step)}<li>{step}</li>{/each}
        </ol>
        <button class="inline-link" onclick={() => api.openLink(info!.help_url!)}>Open {info.help_label}</button>
      {/if}
      {#if servers}
        <div class="grid">
          <label for="imap-host">IMAP server</label>
          <input id="imap-host" class="mono" placeholder={info.imap_host} bind:value={imapHost} autocomplete="off" spellcheck="false" />
          <label for="imap-port">Port</label>
          <input id="imap-port" class="mono port" type="number" min="1" max="65535" bind:value={imapPort} />
          <label for="smtp-host">SMTP server</label>
          <input id="smtp-host" class="mono" placeholder={info.smtp_host} bind:value={smtpHost} autocomplete="off" spellcheck="false" />
          <label for="smtp-port">Port</label>
          <input id="smtp-port" class="mono port" type="number" min="1" max="65535" bind:value={smtpPort} />
        </div>
        <span class="fine">IMAP uses TLS (usually 993). SMTP 465 uses TLS from the start; 587 upgrades with STARTTLS.</span>
        <label for="user">Username</label>
        <input id="user" placeholder={email.trim()} bind:value={username} autocomplete="off" spellcheck="false" />
      {/if}
      <label for="pw">{appPassword ? 'App password' : 'Password'}</label>
      <input id="pw" type="password" class:mono={appPassword} bind:value={password} />
      <span class="fine">Kept in your system keychain. Hysj checks it logs in before saving anything.</span>
      <div class="row">
        <button class="btn primary" disabled={adding || !password.trim()} onclick={connect}>{adding ? 'Connecting…' : edit ? 'Save' : 'Connect'}</button>
        {#if edit}
          <button class="btn ghost" onclick={() => (reset(), oncancel())}>Cancel</button>
        {:else if info.known}
          <button class="btn ghost" onclick={() => (manual = !manual)}>{manual ? `Use ${info.name}’s settings` : 'Edit settings by hand'}</button>
        {/if}
      </div>
    {/if}
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
