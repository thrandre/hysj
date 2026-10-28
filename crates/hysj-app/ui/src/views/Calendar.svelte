<script lang="ts">
  // From mail to calendar: invites in your mail, answered as proper invite
  // replies (after a few seconds, so Undo really undoes), and events added
  // through the Calendar app, which asks where. Nothing is added on its own.
  import { act, app, toggle } from '../lib/app.svelte.ts';
  import { api } from '../lib/api.ts';
  import { clock } from '../lib/clock.svelte.ts';
  import { shortMonth } from '../lib/findit.ts';
  import type { InviteItem } from '../lib/types.ts';
  import { hhmm, weekday } from '../lib/words.ts';
  import Inline from '../components/Inline.svelte';

  const GRACE = 10;
  type Answer = 'accepted' | 'tentative' | 'declined';

  let invites = $state<InviteItem[] | null>(null);
  /** Answers waiting to go out, with their Undo. */
  let pending = $state<Record<string, { answer: Answer; timer: ReturnType<typeof setTimeout> }>>({});
  let added = $state<Record<string, boolean>>({});

  function load() {
    api.calendar().then(
      (i) => (invites = i),
      (e) => (app.error = String(e)),
    );
  }
  $effect(() => {
    void app.data;
    load();
  });

  let open = $derived((invites ?? []).filter((i) => !i.answer && !pending[i.uid]));
  let bookings = $derived((app.data?.bookings ?? []).filter((b) => b.start_ts !== null && b.start_ts > clock.now));
  let headline = $derived(
    !open.length && !bookings.length
      ? 'Your calendar is up to date.'
      : (open.length ? (open.length === 1 ? 'One invite needs an answer' : `${open.length} invites need an answer`) : 'No invites waiting') +
          (bookings.length ? `, and ${bookings.length === 1 ? 'one booking' : `${bookings.length} bookings`} could go in your calendar.` : '.'),
  );

  const at = (ts: number) => new Date(ts * 1000);
  const detail = (i: InviteItem) =>
    [
      i.all_day ? `${weekday(at(i.start))}, all day` : `${weekday(at(i.start))} ${hhmm(at(i.start))}–${hhmm(at(i.end))}`,
      i.organizer,
      i.location,
    ]
      .filter(Boolean)
      .join(' · ');
  const results: Record<Answer, (i: InviteItem) => string> = {
    accepted: (i) => `Accepted. ${i.organizer} gets your answer.`,
    tentative: () => 'Answered maybe.',
    declined: () => 'Declined. The organiser has been told.',
  };

  function answer(i: InviteItem, a: Answer) {
    const timer = setTimeout(async () => {
      delete pending[i.uid];
      if ((await act('Answering the invite…', () => api.inviteAnswer(i.uid, a))) === undefined) return;
      if (a === 'accepted') void add({ invite: i.uid }, i.uid);
      load();
    }, GRACE * 1000);
    pending[i.uid] = { answer: a, timer };
  }
  function undo(i: InviteItem) {
    clearTimeout(pending[i.uid].timer);
    delete pending[i.uid];
  }
  async function add(ev: { invite?: string; booking?: string }, key: string) {
    if ((await act(null, () => api.calendarAdd(ev))) !== undefined) added[key] = true;
  }
</script>

<div class="column">
  <header>
    <p class="meta">Calendar · invites and bookings found in your mail</p>
    <h1 class="h1">{invites ? headline : 'Looking for invites…'}</h1>
  </header>

  {#if invites}
    {#if invites.length}
      <section aria-labelledby="h-inv">
        <h2 id="h-inv">Invites</h2>
        {#each invites as i (i.uid)}
          {@const p = pending[i.uid]}
          <div class="invite card">
            <div class="line">
              <div class="date" aria-hidden="true"><span class="d">{at(i.start).getDate()}</span><span class="m">{shortMonth(at(i.start))}</span></div>
              <div class="text">
                <span class="title">{i.summary}</span>
                <span class="sub">{detail(i)}</span>
                {#if i.clash && !i.answer}<span class="clash"><span class="dot" aria-hidden="true"></span>{i.clash}</span>{/if}
              </div>
            </div>
            {#if p}
              <div class="indent result">
                <span>{p.answer === 'accepted' ? 'Accepting' : p.answer === 'tentative' ? 'Answering maybe' : 'Declining'} in a few seconds.</span>
                <button class="btn small" onclick={() => undo(i)}>Undo</button>
              </div>
            {:else if i.answer}
              <div class="indent result">
                <span>{results[i.answer as Answer]?.(i) ?? 'Answered.'}</span>
                {#if i.answer !== 'declined'}
                  {#if added[i.uid]}<span class="sub">Opened in Calendar.</span>
                  {:else}<button class="btn small" onclick={() => add({ invite: i.uid }, i.uid)}>Add to calendar</button>{/if}
                {/if}
              </div>
            {:else}
              <div class="indent actions" role="group" aria-label="Answer the invite">
                <button class="btn primary" onclick={() => answer(i, 'accepted')}>Accept</button>
                <button class="btn" onclick={() => answer(i, 'tentative')}>Maybe</button>
                <button class="btn" onclick={() => answer(i, 'declined')}>Decline</button>
                <button class="btn ghost" aria-expanded={app.open === i.message} onclick={() => toggle(i.message)}>{app.open === i.message ? 'Close' : 'Open'}</button>
              </div>
            {/if}
            {#if app.open === i.message}<div class="indent"><Inline id={i.message} /></div>{/if}
          </div>
        {/each}
      </section>
    {/if}

    {#if bookings.length}
      <section aria-labelledby="h-book">
        <h2 id="h-book">From your bookings</h2>
        <div class="card list">
          {#each bookings as b (b.key)}
            <div class="row">
              <div class="date small" aria-hidden="true"><span class="d">{at(b.start_ts!).getDate()}</span><span class="m">{shortMonth(at(b.start_ts!))}</span></div>
              <div class="text">
                <span class="name">{b.title}</span>
                <span class="sub">{[b.start?.includes('T') ? `${weekday(at(b.start_ts!))} ${hhmm(at(b.start_ts!))}` : weekday(at(b.start_ts!)), b.provider, b.reference].filter(Boolean).join(' · ')}</span>
              </div>
              {#if added[b.key]}<span class="sub ok">Opened in Calendar</span>
              {:else}<button class="btn small" onclick={() => add({ booking: b.key }, b.key)}>Add to calendar</button>{/if}
            </div>
          {/each}
        </div>
      </section>
    {/if}

    {#if !invites.length && !bookings.length}
      <p class="empty">No invites in your mail, and no bookings coming up.</p>
    {/if}

    <p class="foot">
      Answers go to the organiser from the address the invite came to. Hysj never adds anything on its own: “Add to calendar” opens the Calendar app,
      which asks which calendar to use.
    </p>
  {/if}
</div>

<style>
  header {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-bottom: 28px;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-bottom: 28px;
  }
  h2 {
    margin: 0;
    padding: 0 4px;
    font-size: 13px;
    font-weight: 500;
    color: var(--muted);
  }
  .invite {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 20px 22px;
    border-radius: 22px;
  }
  .line {
    display: flex;
    align-items: flex-start;
    gap: 16px;
  }
  .date {
    flex: none;
    width: 52px;
    display: flex;
    flex-direction: column;
    align-items: center;
  }
  .d {
    font-size: 22px;
    font-weight: 600;
    line-height: 1;
  }
  .date.small .d {
    font-size: 20px;
  }
  .m {
    font-size: 12px;
    color: var(--muted);
  }
  .text {
    flex: 1 1 240px;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .title {
    font-size: 17px;
    font-weight: 600;
  }
  .name {
    font-weight: 600;
  }
  .sub {
    font-size: 13px;
    color: var(--secondary);
  }
  .clash {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    color: var(--warn);
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
  }
  .indent {
    padding-left: 68px;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .result {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
    color: var(--ok);
  }
  .list {
    display: flex;
    flex-direction: column;
    padding: 6px;
    border-radius: 20px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 16px;
    padding: 12px 14px;
  }
  .ok {
    color: var(--ok);
  }
  .foot {
    margin: 0;
    font-size: 13px;
    line-height: 1.6;
    color: var(--muted);
  }
  @media (max-width: 560px) {
    .indent {
      padding-left: 0;
    }
  }
</style>
