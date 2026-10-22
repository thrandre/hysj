<script lang="ts">
  // A code the moment it arrives (Desk mockup): bottom-left, above the nav.
  import { app, copy } from '../lib/app.svelte.ts';
  import { clock } from '../lib/clock.svelte.ts';
  import { grouped } from '../lib/hits.ts';

  let c = $derived(app.data?.codes.find((c) => c.id === app.popup && c.expires > clock.now));
</script>

{#if c}
  <div class="popup card" role="status" aria-label="New sign-in code">
    <div class="row">
      <span class="what">{c.kind} · {c.service}</span>
      <button class="x" aria-label="Dismiss" onclick={() => (app.popup = null)}>×</button>
    </div>
    <div class="row">
      <span class="digits mono">{grouped(c.code ?? '')}</span>
      <button class="btn primary" onclick={() => copy(c!.code ?? '', `code:${c!.id}`)}>{app.copied === `code:${c.id}` ? 'Copied' : 'Copy'}</button>
    </div>
    <span class="fine">Expires in {Math.ceil((c.expires - clock.now) / 60)} min · then deleted</span>
  </div>
{/if}

<style>
  .popup {
    position: fixed;
    left: 24px;
    bottom: 96px;
    z-index: 30;
    width: 320px;
    padding: 16px 18px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    box-shadow: var(--shadow-lg);
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .what {
    font-size: 13px;
    color: var(--secondary);
  }
  .x {
    width: 28px;
    height: 28px;
    border: 0;
    background: none;
    color: var(--muted);
    font-size: 16px;
  }
  .digits {
    font-size: 28px;
    font-weight: 500;
    letter-spacing: 0.08em;
  }
  .popup .btn {
    min-height: 40px;
  }
  .fine {
    font-size: 12px;
    color: var(--muted);
  }
</style>
