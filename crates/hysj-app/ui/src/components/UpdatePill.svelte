<script lang="ts">
  // "Update ready" in the top strip once an update has downloaded: restart
  // now, or let it install when Hysj quits.
  import { app, go } from '../lib/app.svelte.ts';
  import { api } from '../lib/api.ts';

  let open = $state(false);
  let later = $state(false);
  let restarting = $state(false);

  function restart() {
    restarting = true;
    void api.updateRestart();
  }
</script>

{#if app.update?.ready && !later}
  <div class="wrap">
    <button class="pill" aria-expanded={open} onclick={() => (open = !open)}>
      <span class="dot" aria-hidden="true"></span>{restarting ? 'Restarting…' : 'Update ready'}
    </button>
    {#if open}
      <div class="pop" role="dialog" aria-label="Update ready">
        <strong>Hysj {app.update.ready} is ready</strong>
        {#if app.update.notes}<p>{app.update.notes}</p>{/if}
        <span class="fine">Restart takes a few seconds. Your mail keeps syncing in the background.</span>
        <div class="row">
          <button class="btn small primary" onclick={restart}>Restart now</button>
          <button class="btn small" onclick={() => ((later = true), (open = false))}>When I quit</button>
        </div>
        <button class="link" onclick={() => ((open = false), (app.settingsTab = 'about'), go('settings'))}>What’s new · update settings</button>
      </div>
    {/if}
  </div>
{/if}

<style>
  .wrap {
    position: relative;
  }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-height: 32px;
    padding: 0 12px;
    border: 0;
    border-radius: 999px;
    background: var(--primary);
    color: var(--on-primary);
    font: inherit;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: #7fd3a8;
  }
  .pop {
    position: absolute;
    right: 0;
    top: 42px;
    z-index: 20;
    width: 320px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 18px;
    border-radius: 18px;
    background: var(--surface);
    box-shadow: var(--shadow-lg);
    color: var(--ink);
    font-size: 14px;
    text-align: left;
  }
  .pop strong {
    font-size: 16px;
  }
  .pop p {
    margin: 0;
    line-height: 1.55;
    color: var(--body);
  }
  .fine {
    font-size: 12px;
    color: var(--muted);
  }
  .row {
    display: flex;
    gap: 8px;
  }
  .link {
    align-self: flex-start;
    padding: 0;
    border: 0;
    background: none;
    font: inherit;
    font-size: 13px;
    color: var(--secondary);
    text-decoration: underline;
    cursor: pointer;
  }
</style>
