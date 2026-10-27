<script lang="ts">
  import { app, go, start } from './lib/app.svelte.ts';
  import TopBar from './components/TopBar.svelte';
  import SpacesNav from './components/SpacesNav.svelte';
  import Desk from './views/Desk.svelte';
  import Focus from './views/Focus.svelte';
  import Digest from './views/Digest.svelte';
  import Screener from './views/Screener.svelte';
  import Codes from './views/Codes.svelte';
  import Bookings from './views/Bookings.svelte';
  import Orders from './views/Orders.svelte';
  import Folder from './views/Folder.svelte';
  import Archive from './views/Archive.svelte';
  import Settings from './views/Settings.svelte';
  import Search from './views/Search.svelte';
  import Welcome from './views/Welcome.svelte';
  import Compose from './components/Compose.svelte';
  import CodePopup from './components/CodePopup.svelte';
  import Opening from './components/Opening.svelte';

  start();

  /** Keys that work everywhere: ⌘K the command bar, ⌘N a new email, Esc closes. */
  function keys(e: KeyboardEvent) {
    const typing = (e.target as HTMLElement | null)?.closest('input, textarea, select, [contenteditable]');
    if (e.metaKey && e.key.toLowerCase() === 'k') {
      e.preventDefault();
      if (app.view !== 'desk') go('desk');
      app.focusCommand++;
    } else if (e.metaKey && e.key.toLowerCase() === 'n') {
      e.preventDefault();
      app.compose ??= { to: '' };
    } else if (e.key === 'Escape' && !typing) {
      if (app.compose) app.compose = null;
      else app.open = null;
    }
  }
</script>

<svelte:window onkeydown={keys} />

<TopBar />
<main>
  {#if app.error}
    <div class="error" role="alert">
      <span>{app.error}</span>
      <button class="btn small ghost" onclick={() => (app.error = null)}>Dismiss</button>
    </div>
  {/if}
  {#if !app.data}
    <Opening />
  {:else if app.view === 'desk'}
    <Desk />
  {:else if app.view === 'focus'}
    <Focus />
  {:else if app.view === 'digest'}
    {#key app.digestPick}<Digest />{/key}
  {:else if app.view === 'screener'}
    <Screener />
  {:else if app.view === 'codes'}
    <Codes />
  {:else if app.view === 'bookings'}
    <Bookings />
  {:else if app.view === 'orders'}
    <Orders />
  {:else if app.view === 'sent'}
    <Folder />
  {:else if app.view === 'done'}
    <Archive />
  {:else if app.view === 'settings'}
    <Settings />
  {:else if app.view === 'search'}
    {#key app.searchQuery}<Search />{/key}
  {/if}
</main>
{#if app.compose}
  <Compose />
{/if}
<CodePopup />
{#if app.data && app.welcome !== null}
  <Welcome />
{/if}
{#if app.data && app.view !== 'focus'}
  <SpacesNav />
{/if}

<style>
  main {
    position: fixed;
    inset: 72px 0 0 0;
    overflow-y: auto;
  }
  .error {
    position: fixed;
    top: 76px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 20;
    width: min(640px, calc(100vw - 32px));
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 8px 8px 16px;
    border-radius: 12px;
    background: var(--error-bg);
    color: var(--error);
  }
  .error .btn {
    color: var(--error);
  }
  :global(.column) {
    max-width: 760px;
    margin: 0 auto;
    padding: 32px 24px 160px;
  }
</style>
