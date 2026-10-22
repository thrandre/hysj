<script lang="ts">
  import { act, app, copy } from '../lib/app.svelte.ts';
  import { api } from '../lib/api.ts';
  import { clock } from '../lib/clock.svelte.ts';
  import { codesHeadline } from '../lib/findit.ts';
  import { grouped } from '../lib/hits.ts';
  import { hhmm } from '../lib/words.ts';

  let live = $derived(app.data!.codes.filter((c) => c.expires > clock.now));
  let expired = $derived(app.data!.codes.filter((c) => c.expires <= clock.now));
  let settings = $derived(app.data!.codeSettings);
  const keep = (h: number) => (h === 1 ? '1 hour' : h === 168 ? '7 days' : '24 hours');
  const at = (ts: number) => hhmm(new Date(ts * 1000));
  const left = (expires: number) => {
    const s = Math.max(0, Math.floor(expires - clock.now));
    return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')} left`;
  };
  const save = (popup: boolean, retention_hours: number) => act(null, () => api.codeSettings({ popup, retention_hours }));
</script>

<div class="column">
  <header>
    <p class="meta">Codes · never in your inbox · deleted when they expire</p>
    <h1 class="h1">{codesHeadline(live.filter((c) => !c.reset).length, live.filter((c) => c.reset).length)}</h1>
  </header>

  {#each live as c (c.id)}
    {#if !c.reset}
      <div class="card code">
        <div class="row"><strong>{c.service} {c.kind}</strong><span class="left mono">{left(c.expires)}</span></div>
        <div class="row">
          <span class="digits mono">{grouped(c.code ?? '')}</span>
          <button class="btn primary big" onclick={() => copy(c.code ?? '', `code:${c.id}`)}>{app.copied === `code:${c.id}` ? 'Copied' : 'Copy'}</button>
        </div>
        <div class="bar" aria-hidden="true">
          <div style:width="{Math.max(0, Math.min(1, (c.expires - clock.now) / Math.max(1, c.expires - c.arrived))) * 100}%"></div>
        </div>
        <span class="fine">Arrived {at(c.arrived)} from {c.sender_domain}</span>
      </div>
    {:else}
      <div class="card reset">
        <div class="row">
          <strong>{c.service} password reset</strong>
          <span class="fine">requested {Math.max(0, Math.round((clock.now - c.arrived) / 60))} min ago · valid {Math.round((c.expires - c.arrived) / 60)} min</span>
        </div>
        {#if c.link_ok}
          <span class="ok">✓ The link goes to {c.service}’s own domain</span>
        {:else if c.link_host}
          <span class="dim">The link goes to {c.link_host}, not {c.sender_domain}</span>
        {/if}
        {#if c.reported}
          <p class="reported">Link disabled. Consider changing your {c.service} password and turning on two-factor sign-in.</p>
        {:else}
          <div class="actions">
            {#if c.link}<button class="btn primary" onclick={() => api.openLink(c.link!)}>Open reset link</button>{/if}
            <button class="btn danger" onclick={() => act(null, () => api.codeReport(c.id))}>I didn’t ask for this</button>
          </div>
        {/if}
      </div>
    {/if}
  {:else}
    <p class="empty">Codes pop up here the moment they arrive.</p>
  {/each}

  {#if expired.length}
    <section aria-labelledby="h-exp">
      <h2 id="h-exp">Earlier — gone for good after {keep(settings.retention_hours)}</h2>
      {#each expired as c (c.id)}
        <div class="expired">
          <span class="what">{c.service} · {c.kind}</span>
          <span class="mono">{c.code ? `••• •${c.code.slice(-2)}` : '—'}</span>
          <span class="status">{c.reported ? 'reported' : `expired ${at(c.expires)}`}</span>
        </div>
      {/each}
    </section>
  {/if}

  <section class="settings" aria-label="Code settings">
    <label><input type="checkbox" checked={settings.popup} onchange={(e) => save(e.currentTarget.checked, settings.retention_hours)} /> Show codes the moment they arrive, even in focus time</label>
    <label class="off"><input type="checkbox" checked disabled /> Keep codes on this computer only — never sync them</label>
    <label>
      Forget expired codes after
      <select value={settings.retention_hours} onchange={(e) => save(settings.popup, Number(e.currentTarget.value))}>
        <option value={1}>1 hour</option>
        <option value={24}>24 hours</option>
        <option value={168}>7 days</option>
      </select>
    </label>
  </section>
</div>

<style>
  header {
    margin-bottom: 32px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 18px;
    margin-bottom: 18px;
  }
  .code {
    padding: 28px;
    border-radius: 24px;
    box-shadow: var(--shadow-md);
  }
  .reset {
    padding: 24px;
    gap: 14px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .left {
    font-size: 13px;
    color: var(--accent);
  }
  .digits {
    font-size: 56px;
    font-weight: 500;
    letter-spacing: 0.1em;
    line-height: 1;
    user-select: text;
    -webkit-user-select: text;
  }
  .big {
    height: 52px;
    padding: 0 26px;
    font-size: 16px;
  }
  .bar {
    height: 4px;
    border-radius: 2px;
    background: var(--line);
    overflow: hidden;
  }
  .bar div {
    height: 100%;
    background: var(--accent);
  }
  .fine,
  .dim {
    font-size: 13px;
    color: var(--muted);
  }
  .ok {
    color: var(--ok);
  }
  .reported {
    margin: 0;
    padding: 12px 14px;
    border-radius: 12px;
    background: var(--error-bg);
    color: var(--error);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  h2 {
    margin: 12px 0 8px;
    font-size: 13px;
    font-weight: 500;
    color: var(--muted);
  }
  .expired {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 16px;
    padding: 10px 0;
    border-bottom: 1px solid var(--raised);
    color: var(--secondary);
  }
  .expired .what {
    flex: 1 1 200px;
    color: var(--ink);
  }
  .expired .mono {
    font-size: 13px;
  }
  .status {
    width: 120px;
    text-align: right;
    font-size: 13px;
  }
  .settings {
    margin-top: 28px;
    display: flex;
    flex-direction: column;
  }
  .settings label {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 44px;
  }
  .settings input[type='checkbox'] {
    width: 20px;
    height: 20px;
    accent-color: var(--ink);
  }
  .settings .off {
    color: var(--muted);
  }
  select {
    height: 36px;
    padding: 0 10px;
    border: 0;
    border-radius: 10px;
    background: var(--raised);
  }
</style>
