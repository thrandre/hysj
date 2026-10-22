<script lang="ts">
  import { act, accountSlot, app, toggle, visible } from '../lib/app.svelte.ts';
  import { api } from '../lib/api.ts';
  import Inline from '../components/Inline.svelte';
  import Conversation from '../components/Conversation.svelte';
  import CommandBar from '../components/CommandBar.svelte';

  let bar: CommandBar | undefined = $state();
  import { age, firstName, initials, peopleHeadline, subject, weekday, hhmm } from '../lib/words.ts';
  import type { MessageSummary, Signal } from '../lib/types.ts';

  // "Can't wait for the digest": what each kind is and does.
  const KINDS: Record<string, { why: string; action: string; name: string; urgent: boolean }> = {
    payment: { why: 'Money · a payment didn’t go through', action: 'Fix payment', name: 'Failed payments', urgent: true },
    security: { why: 'Security · check this sign-in or device', action: 'Check activity', name: 'Security alerts', urgent: true },
    review: { why: 'Code review · you’re the requested reviewer', action: 'Open review', name: 'Review requests', urgent: false },
  };
  let signals = $derived(app.data!.signals);

  async function hold(g: Signal) {
    const k = KINDS[g.kind];
    if ((await act(null, () => api.signalHold(g.kind, true))) === undefined) return;
    app.learned = {
      text: `${k.name} now wait for the digest.`,
      undo: () => act(null, () => api.signalHold(g.kind, false)).then(() => (app.learned = null)),
    };
  }

  let now = $state(Date.now());
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 30_000);
    return () => clearInterval(t);
  });

  let people = $derived(visible(app.data!.people));
  let waiting = $derived(people.filter((m) => m.intent === 'reply'));
  let also = $derived(people.filter((m) => m.intent !== 'reply'));
  const gist = (m: MessageSummary) => m.gist ?? subject(m.subject);
  let open = $derived(people.find((m) => m.id === app.open));
</script>

<div class="column">
  <p class="meta">{weekday(new Date(now))} · {hhmm(new Date(now))}</p>
  <CommandBar bind:this={bar} />
  {#if !bar?.searching()}
  {#if app.learned}
    <div class="learned" role="status">
      <span>Learned: {app.learned.text}</span>
      <button onclick={() => app.learned?.undo()}>Undo</button>
    </div>
  {/if}
  {#if signals.length}
    <section class="signals" aria-labelledby="h-sig">
      <div class="sig-head">
        <h2 id="h-sig">Can’t wait for the digest</h2>
        <span class="mono">{signals.length}</span>
      </div>
      {#each signals as g (g.id)}
        {@const k = KINDS[g.kind]}
        <div class="signal">
          <div class="sig-top">
            <span class="dot" class:urgent={k.urgent} aria-hidden="true"></span>
            <div class="sig-text">
              <span class="sig-title">{g.title}</span>
              <span class="sig-why">{g.from} · {k.why}{g.ids.length > 1 ? ` · ${g.ids.length} emails` : ''}</span>
            </div>
            <span class="mono sig-when">{g.ts ? age(g.ts, now / 1000) : ''}</span>
          </div>
          <div class="sig-actions">
            <button class="btn small primary" onclick={() => toggle(g.id)} aria-expanded={app.open === g.id}>{k.action}</button>
            <button class="btn small" onclick={() => act(null, () => api.done(g.ids))}>Done</button>
            <button class="btn small ghost" onclick={() => hold(g)}>Hold these for the digest</button>
          </div>
          {#if app.open === g.id}<Inline id={g.id} />{/if}
        </div>
      {/each}
    </section>
  {/if}
  <h1>{peopleHeadline(waiting.length, new Date(now).getHours())}</h1>

  <ul class="people">
    {#each waiting as m (m.id)}
      {@const slot = accountSlot(m.account)}
      {@const long = !m.replied && m.ts !== null && now / 1000 - m.ts > 86400}
      <li>
        <button class="person" class:open={app.open === m.id} aria-pressed={app.open === m.id} onclick={() => toggle(m.id)} title={m.subject}>
          <span class="avatar" style:background="var(--acct-{slot}-bg)" aria-hidden="true">
            {initials(m.from)}<span class="acct" style:background="var(--acct-{slot})"></span>
          </span>
          <span class="text">
            <span class="name">{m.from}</span>
            <span class="gist">{gist(m)}</span>
          </span>
          <span class="wait mono" class:late={long}>{m.ts ? age(m.ts, now / 1000) : ''}</span>
        </button>
      </li>
    {/each}
  </ul>

  {#if also.length}
    <section class="also" aria-label="Also today">
      <span class="label">Also today</span>
      <p>
        {#each also as m, i (m.id)}
          <button class="name-link" onclick={() => toggle(m.id)}>{firstName(m.from)}</button> — {gist(m).replace(/\.$/, '')}.{i + 1 < also.length ? ' ' : ''}
        {/each}
      </p>
    </section>
  {/if}
  {/if}
</div>

{#if open}
  {#key open.id}
    <Conversation m={open} />
  {/key}
{/if}

<style>
  .signals {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 28px 0 36px;
  }
  .sig-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 12px;
    color: var(--muted);
    font-size: 12px;
  }
  .sig-head h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 500;
    color: var(--muted);
  }
  .signal {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 16px;
    border-radius: 16px;
    background: var(--surface);
    box-shadow: var(--shadow-sm);
  }
  .sig-top {
    display: flex;
    align-items: flex-start;
    gap: 12px;
  }
  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    margin-top: 7px;
    border-radius: 50%;
    background: var(--body);
  }
  .dot.urgent {
    background: var(--accent);
  }
  .sig-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .sig-title {
    font-weight: 600;
    line-height: 1.4;
  }
  .sig-why {
    font-size: 13px;
    color: var(--secondary);
  }
  .sig-when {
    flex: none;
    font-size: 12px;
    color: var(--muted);
  }
  .sig-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    padding-left: 20px;
  }
  .learned {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin: 28px 0 0;
    padding: 12px 16px;
    border-radius: 14px;
    background: var(--ok-bg);
    color: var(--ok);
  }
  .learned span {
    flex: 1 1 260px;
  }
  .learned button {
    min-height: 32px;
    padding: 0 12px;
    border: 0;
    border-radius: 999px;
    background: var(--surface);
    font: inherit;
    font-size: 13px;
    color: var(--ink);
    cursor: pointer;
  }
  .meta {
    margin: 0 0 14px;
  }
  h1 {
    margin: 36px 0 18px;
    font-size: 30px;
    line-height: 1.2;
    font-weight: 600;
    letter-spacing: -0.02em;
  }
  .people {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .person {
    display: flex;
    align-items: center;
    gap: 16px;
    width: calc(100% + 32px);
    margin: 0 -16px;
    padding: 14px 16px;
    border: 0;
    border-radius: 16px;
    background: transparent;
    text-align: left;
  }
  .person:hover {
    background: color-mix(in srgb, var(--surface) 55%, transparent);
  }
  .person.open {
    background: var(--surface);
    box-shadow: var(--shadow-sm);
  }
  .avatar {
    position: relative;
    flex: none;
    width: 44px;
    height: 44px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 600;
    font-size: 14px;
  }
  .acct {
    position: absolute;
    right: -1px;
    bottom: -1px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: 2px solid var(--ground);
  }
  .text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .name {
    font-weight: 600;
  }
  .gist {
    font-size: 17px;
    line-height: 1.4;
    color: var(--body);
  }
  .wait {
    flex: none;
    font-size: 12px;
    color: var(--muted);
  }
  .wait.late {
    color: var(--accent);
  }
  .also {
    margin-top: 36px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .also .label {
    font-size: 13px;
    color: var(--muted);
  }
  .also p {
    margin: 0;
    font-size: 16px;
    line-height: 1.6;
    color: var(--body);
  }
  .name-link {
    /* A link-sized word with a finger-sized target. */
    padding: 6px 2px;
    margin: -6px -2px;
    border: 0;
    background: none;
    color: var(--ink);
    font-size: inherit;
    text-decoration: underline;
    text-underline-offset: 3px;
  }
</style>
