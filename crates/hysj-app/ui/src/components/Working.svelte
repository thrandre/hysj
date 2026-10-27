<script lang="ts">
  // Shows that an action is under way when it takes a moment, so a slow
  // click never looks like a stuck app.
  import { app } from '../lib/app.svelte.ts';
  import { clock } from '../lib/clock.svelte.ts';

  /** After this long it says why it's still going. */
  const SLOW = 8;

  let job = $derived(app.working[0]);
  let more = $derived(app.working.length - 1);
  let slow = $derived(job !== undefined && clock.now - job.since >= SLOW);
</script>

<span class="working" role="status" aria-live="polite">
  {#if job}
    <span class="pill">
      <span class="spinner" aria-hidden="true"></span>
      {slow ? 'Still working — this is taking longer than usual.' : job.label}{more ? ` and ${more} more` : ''}
    </span>
  {/if}
</span>

<style>
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-height: 32px;
    padding: 0 12px;
    border-radius: 999px;
    background: var(--raised);
    color: var(--body);
    font-size: 13px;
    white-space: nowrap;
    /* Quick actions finish before it shows: no flicker. */
    opacity: 0;
    animation: show 0.15s ease-out 0.3s forwards;
  }
  .spinner {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: 2px solid var(--muted);
    border-right-color: transparent;
    animation: spin 0.8s linear infinite;
  }
  @keyframes show {
    to {
      opacity: 1;
    }
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .spinner {
      animation: none;
      border-right-color: var(--muted);
    }
  }
</style>
