<script lang="ts">
  import { act, app, visible } from '../lib/app.svelte.ts';
  import { api } from '../lib/api.ts';
  import type { Decision, MessageSummary } from '../lib/types.ts';
  import { initials, reasonText, subject } from '../lib/words.ts';

  let senders = $derived.by(() => {
    const out: [string, MessageSummary, number][] = [];
    for (const m of visible(app.data!.screener)) {
      const at = out.find(([a]) => a === m.from_addr.toLowerCase());
      if (at) at[2]++;
      else out.push([m.from_addr.toLowerCase(), m, 1]);
    }
    return out;
  });
  let headline = $derived(
    senders.length === 0 ? 'Everyone has a place now.' : senders.length === 1 ? 'One new sender is waiting at the door.' : `${senders.length} new senders are waiting at the door.`,
  );
  const results: Record<string, string> = {
    people: 'Let in — their mail now lands on your Desk.',
    digest: 'Sent to the Digest — you’ll see them at 08:00 and 16:00.',
    blocked: 'Blocked — you won’t hear from them again.',
  };

  async function decide(sender: string, m: MessageSummary, d: Decision) {
    const r = await act(null, () => api.screenerDecide(sender, d));
    if (r) app.decided = [{ sender, bucket: r.bucket, name: m.from, subject: m.subject }, ...app.decided.filter((x) => x.sender !== sender)];
  }
  async function undo(sender: string) {
    app.decided = app.decided.filter((x) => x.sender !== sender);
    await act(null, () => api.screenerForget(sender));
  }
</script>

<div class="column">
  <header>
    <p class="meta">Screener · all accounts</p>
    <h1 class="h1">{headline}</h1>
    <p class="lede">They never notify you. Decide once where each one belongs — Hysj remembers it for every account.</p>
  </header>

  <div class="list">
    {#each app.decided as d (d.sender)}
      <div class="card sender">
        <div class="head">
          <span class="avatar" aria-hidden="true">{initials(d.name)}</span>
          <div class="who">
            <span><strong>{d.name}</strong> <span class="addr">{d.sender}</span></span>
            {#if d.subject}<span class="subject">{subject(d.subject)}</span>{/if}
          </div>
        </div>
        <div class="row decided">
          <span class="ok">{results[d.bucket] ?? results.blocked}</span>
          <button class="btn small" onclick={() => undo(d.sender)}>Undo</button>
        </div>
      </div>
    {/each}
    {#each senders as [addr, m, n] (addr)}
      <div class="card sender">
        <div class="head">
          <span class="avatar" aria-hidden="true">{initials(m.from)}</span>
          <div class="who">
            <span><strong>{m.from}</strong> <span class="addr">{m.from_addr}</span></span>
            <span class="subject">{subject(m.subject)}</span>
            <span class="hint">{reasonText(m.reason)}{n > 1 ? ` · ${n} emails` : ''}</span>
          </div>
        </div>
        <div class="row" role="group" aria-label="Where should {m.from} go?">
          <button class="btn primary" onclick={() => decide(addr, m, 'allow')}>Let them in</button>
          <button class="btn" onclick={() => decide(addr, m, 'digest')}>Digest</button>
          <button class="btn danger push" onclick={() => decide(addr, m, 'block')}>Block</button>
        </div>
      </div>
    {:else}
      {#if !app.decided.length}<p class="empty">Nobody new. Enjoy the quiet.</p>{/if}
    {/each}
  </div>
  <p class="foot">
    People skip the Screener when you’ve written to them before, or when someone you’ve written to introduces them. Newsletters and automated mail go
    straight to the Digest.
  </p>
</div>

<style>
  header {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-bottom: 28px;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .sender {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 20px 22px;
  }
  .head {
    display: flex;
    gap: 14px;
  }
  .avatar {
    flex: none;
    width: 44px;
    height: 44px;
    border-radius: 50%;
    background: var(--raised);
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 600;
    font-size: 14px;
  }
  .who {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .addr,
  .hint {
    font-size: 13px;
    color: var(--muted);
  }
  .subject {
    font-size: 17px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    padding-left: 58px;
  }
  .push {
    margin-left: auto;
  }
  .decided {
    align-items: center;
    justify-content: space-between;
  }
  .ok {
    color: var(--ok);
    font-weight: 500;
  }
  .foot {
    margin-top: 28px;
    font-size: 14px;
    line-height: 1.6;
    color: var(--secondary);
  }
</style>
