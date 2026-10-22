<script lang="ts">
  // The first run (Welcome mockup): add your mail, pick digest times, sort
  // the mail, then the Desk. Every count shown is the real one, after the
  // first sync. Shown while app.welcome is a step number.
  import { act, accountSlot, app, go } from '../lib/app.svelte.ts';
  import { api } from '../lib/api.ts';
  import AccountForm from '../components/AccountForm.svelte';
  import DigestHours from '../components/DigestHours.svelte';

  let step = $derived(app.welcome ?? 0);
  let accounts = $derived(app.data?.accounts ?? []);
  /** The first sync is done (step 3 waits for it). */
  let sorted = $state(false);

  const n = (x: number) => x.toLocaleString('en-GB');

  async function sortMail() {
    app.welcome = 3;
    sorted = false;
    await act(null, () => api.sync());
    sorted = true;
  }

  function finish() {
    app.welcome = null;
    go('desk');
  }

  let d = $derived(app.data);
  let total = $derived(d ? d.people.length + d.digest.length + d.screener.length + d.done.length : 0);
  let waiting = $derived(d ? d.people.filter((m) => m.intent === 'reply').length : 0);
  let tasks = $derived([
    ['Reading your mail', `${n(total)} emails`],
    ['Finding the people you write to', `${n(d?.contacts.length ?? 0)} people`],
    ['Holding newsletters and notifications', `${n(d?.digest.length ?? 0)} held`],
    ['Pulling out bookings, orders and codes', `${n((d?.bookings.length ?? 0) + (d?.orders.length ?? 0) + (d?.codes.length ?? 0))} found`],
    ['The on-device AI', 'downloads when it’s first needed'],
  ]);
</script>

<div class="welcome">
  <div class="strip" data-tauri-drag-region>
    {#if step > 0 && step < 4}
      <span class="mark">Hysj!</span>
      <div class="dots" role="img" aria-label="Step {step} of 3">
        {#each [1, 2, 3] as i (i)}<span class:done={i < step} class:now={i === step}></span>{/each}
      </div>
    {/if}
  </div>

  <div class="stage">
    {#if step === 0}
      <div class="center">
        <span class="logo">Hysj!</span>
        <p class="tag">Only people reach you.<br />Everything else waits.</p>
        <button class="btn primary big" onclick={() => (app.welcome = 1)}>Get started</button>
        <span class="fine">Your mail and Hysj’s AI stay on this computer.</span>
      </div>
    {:else if step === 1}
      <div class="panel">
        <h1>Add your mail.</h1>
        <p class="lede">Outlook.com, iCloud, your own domain — or several. They all land on one Desk.</p>
        {#each accounts as a (a.name)}
          <div class="added card">
            <span class="dot" style:background="var(--acct-{accountSlot(a.name)})" aria-hidden="true"></span>
            <strong>{a.address}</strong>
            <span class="ok">{a.host === 'imap.gmail.com' ? 'Gmail' : 'IMAP'} · connected</span>
          </div>
        {/each}
        <AccountForm title={accounts.length ? 'Add another address' : 'Email address'} />
        <div class="nav">
          <button class="btn ghost" onclick={() => (app.welcome = 0)}>Back</button>
          <button class="btn primary big" disabled={!accounts.length} onclick={() => (app.welcome = 2)}>Continue</button>
        </div>
      </div>
    {:else if step === 2}
      <div class="panel">
        <h1>When should the rest arrive?</h1>
        <p class="lede">People reach you right away. Newsletters, notifications and receipts wait for your digest. Anything that can’t wait still comes through.</p>
        <DigestHours />
        <div class="nav">
          <button class="btn ghost" onclick={() => (app.welcome = 1)}>Back</button>
          <button class="btn primary big" onclick={sortMail}>Sort my mail</button>
        </div>
      </div>
    {:else if step === 3}
      <div class="panel">
        <h1>{sorted ? 'Done. Your mail is sorted.' : 'Sorting your mail…'}</h1>
        <p class="lede">Everything happens on this computer. Nothing is sent anywhere.</p>
        <div class="bar" class:busy={!sorted} aria-hidden="true"><div></div></div>
        <ol class="tasks" aria-live="polite">
          {#each tasks as [label, value] (label)}
            <li>
              <span class="tick" class:done={sorted} aria-hidden="true">{sorted ? '✓' : ''}</span>
              <span class="what" class:dim={!sorted}>{label}</span>
              <span class="mono value">{sorted ? value : '…'}</span>
            </li>
          {/each}
        </ol>
        <div class="nav end">
          <button class="btn primary big" disabled={!sorted} onclick={() => (app.welcome = 4)}>{sorted ? 'See what reached you' : 'Almost there'}</button>
        </div>
      </div>
    {:else}
      <div class="center">
        <h1 class="quiet">It’s quiet now.</h1>
        <p class="tag small">
          Out of {n(total)} emails, {waiting === 1 ? 'one person is' : `${n(waiting)} people are`} waiting on you.
          {n(d?.digest.length ?? 0)} newsletters and notifications will wait for your digest.
        </p>
        <button class="btn primary big" onclick={finish}>Open your Desk</button>
        <div class="hints">
          <span><kbd class="mono">⌘K</kbd> ask or do anything</span>
          <span>“Why here?” on every email — correct it once and Hysj remembers</span>
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .welcome {
    position: fixed;
    inset: 0;
    z-index: 30;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    background: var(--ground);
  }
  .strip {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-height: 40px;
    padding: 20px 28px 20px 96px;
  }
  .mark {
    font-weight: 700;
    font-size: 18px;
    letter-spacing: -0.02em;
  }
  .dots {
    display: flex;
    gap: 6px;
  }
  .dots span {
    width: 28px;
    height: 4px;
    border-radius: 2px;
    background: var(--raised);
  }
  .dots .now {
    background: var(--muted);
  }
  .dots .done {
    background: var(--ink);
  }
  .stage {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px 24px 64px;
  }
  .center {
    max-width: 640px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 22px;
    text-align: center;
  }
  .logo {
    font-size: 120px;
    line-height: 1;
    font-weight: 700;
    letter-spacing: -0.06em;
  }
  .tag {
    margin: 0;
    font-size: 26px;
    line-height: 1.35;
    font-weight: 500;
    letter-spacing: -0.015em;
    color: var(--body);
  }
  .tag.small {
    font-size: 19px;
    font-weight: 400;
    line-height: 1.55;
  }
  .quiet {
    margin: 0;
    font-size: 48px;
    line-height: 1.1;
    font-weight: 600;
    letter-spacing: -0.035em;
  }
  .big {
    height: 56px;
    padding: 0 30px;
    border-radius: 16px;
    font-size: 17px;
  }
  .fine {
    font-size: 13px;
    color: var(--muted);
  }
  .panel {
    width: 100%;
    max-width: 560px;
    display: flex;
    flex-direction: column;
    gap: 22px;
  }
  .panel h1 {
    margin: 0;
    font-size: 34px;
    line-height: 1.15;
    font-weight: 600;
    letter-spacing: -0.025em;
  }
  .lede {
    margin: -14px 0 0;
    font-size: 17px;
    line-height: 1.6;
    color: var(--body);
  }
  .added {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 14px 16px;
    border-radius: 16px;
  }
  .added strong {
    flex: 1;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }
  .ok {
    font-size: 13px;
    color: var(--ok);
  }
  .nav {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
  }
  .nav.end {
    justify-content: flex-end;
  }
  .bar {
    height: 6px;
    border-radius: 3px;
    background: var(--raised);
    overflow: hidden;
  }
  .bar div {
    width: 100%;
    height: 100%;
    background: var(--ink);
  }
  .bar.busy div {
    width: 30%;
    animation: slide 1.2s ease-in-out infinite;
  }
  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(340%);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .bar.busy div {
      animation: none;
      width: 50%;
    }
  }
  .tasks {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .tasks li {
    display: flex;
    align-items: center;
    gap: 14px;
    min-height: 48px;
  }
  .tick {
    flex: none;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--raised);
    color: var(--on-primary);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 13px;
  }
  .tick.done {
    background: var(--ink);
  }
  .what {
    flex: 1;
  }
  .dim {
    color: var(--secondary);
  }
  .value {
    font-size: 13px;
    color: var(--secondary);
  }
  .hints {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 18px;
    padding-top: 8px;
    font-size: 14px;
    color: var(--secondary);
  }
  kbd {
    font-size: 12px;
    padding: 3px 7px;
    border-radius: 6px;
    background: var(--raised);
    color: var(--ink);
  }
</style>
