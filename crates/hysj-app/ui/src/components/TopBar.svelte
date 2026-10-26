<script lang="ts">
  import { accountLabel, app, digestNow, digestSplit, go, nextDigest } from '../lib/app.svelte.ts';
  import { earlier } from '../lib/digest.ts';
  import UpdatePill from './UpdatePill.svelte';

  let next = $derived(nextDigest());
  let held = $derived(app.data ? digestSplit()[1].length : 0);
</script>

<!-- The window's title bar: empty space drags the window (macOS buttons sit at the left). -->
<header data-tauri-drag-region>
  <button class="mark" onclick={() => go('desk')} title="Desk">Hysj!</button>

  {#if app.view === 'focus'}
    <div class="progress" role="img" aria-label="{Math.min(app.focus.at + 1, app.focus.queue.length)} of {app.focus.queue.length}" data-tauri-drag-region>
      {#each app.focus.queue.slice(0, 24) as _, i (i)}
        <span class:done={i < app.focus.at} class:now={i === app.focus.at}></span>
      {/each}
    </div>
  {/if}

  <nav class="links" aria-label="This space" data-tauri-drag-region>
    {#if app.busy}
      <span class="busy">{app.busy}</span>
    {:else if app.view === 'desk' && app.data}
      <UpdatePill />
      <span class="held"><span class="dot" aria-hidden="true"></span>{held} held for {next}</span>
      {#if app.data.accounts.length > 1}
        <div class="segmented" role="group" aria-label="Accounts">
          <button aria-pressed={app.account === null} onclick={() => (app.account = null)}>All</button>
          {#each app.data.accounts as a (a.name)}
            <button aria-pressed={app.account === a.name} onclick={() => (app.account = a.name)}>{accountLabel(a.name)}</button>
          {/each}
        </div>
      {/if}
      <button class="link" onclick={() => go('focus')}>One at a time</button>
    {:else if app.view === 'digest'}
      <div class="segmented" role="group" aria-label="Digest editions">
        <button aria-pressed={app.digestPick === 0} onclick={() => (app.digestPick = 0)}>Latest</button>
        {#each earlier(digestNow()) as [pick, label] (pick)}
          <button aria-pressed={app.digestPick === pick} onclick={() => (app.digestPick = pick)}>{label}</button>
        {/each}
      </div>
    {:else if app.view === 'bookings'}
      <button class="link" onclick={() => (app.showPast = !app.showPast)}>{app.showPast ? 'Upcoming bookings' : 'Past bookings'}</button>
    {:else if app.view === 'orders'}
      <div class="segmented" role="group" aria-label="Filter orders">
        {#each ['On the way', 'Delivered', 'Returns'] as label, i (label)}
          <button aria-pressed={app.orderTab === i} onclick={() => (app.orderTab = i)}>{label}</button>
        {/each}
      </div>
    {/if}
    {#if app.view === 'focus'}
      <button class="pill" onclick={() => go('desk')}>Back to Desk</button>
    {:else if ['sent', 'done', 'settings', 'search'].includes(app.view)}
      <button class="link" onclick={() => go('desk')}>Back to Desk</button>
    {:else}
      <button class="link" onclick={() => go('done')}>Archive</button>
      <button class="link" onclick={() => go('settings')}>Settings</button>
    {/if}
  </nav>
</header>

<style>
  header {
    position: fixed;
    inset: 0 0 auto 0;
    height: 72px;
    z-index: 10;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    /* Room for the macOS window buttons. */
    padding: 0 28px 0 104px;
    background: var(--ground);
  }
  .mark {
    border: 0;
    background: none;
    padding: 8px 0;
    font-weight: 700;
    font-size: 18px;
    letter-spacing: -0.02em;
  }
  .progress {
    position: absolute;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    gap: 6px;
  }
  .progress span {
    width: 28px;
    height: 4px;
    border-radius: 2px;
    background: var(--raised);
  }
  .progress span.done {
    background: var(--ink);
  }
  .progress span.now {
    background: var(--muted);
  }
  .links {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 16px;
    font-size: 13px;
    color: var(--secondary);
  }
  .link {
    min-height: 44px;
    padding: 0 10px;
    border: 0;
    border-radius: 999px;
    background: none;
    color: var(--secondary);
    font-size: 13px;
  }
  .link:hover {
    background: var(--raised);
    color: var(--ink);
  }
  .pill {
    min-height: 36px;
    padding: 0 14px;
    border: 0;
    border-radius: 999px;
    background: var(--raised);
    font-size: 13px;
  }
  .held {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
  }
  .busy {
    color: var(--muted);
  }
  .segmented {
    display: flex;
    gap: 2px;
    padding: 3px;
    border-radius: 999px;
    background: var(--raised);
  }
  .segmented button {
    min-height: 30px;
    padding: 0 12px;
    border: 0;
    border-radius: 999px;
    background: transparent;
    font-size: 13px;
  }
  .segmented button[aria-pressed='true'] {
    background: var(--segment);
    box-shadow: 0 1px 3px rgba(17, 17, 17, 0.12);
  }
</style>
