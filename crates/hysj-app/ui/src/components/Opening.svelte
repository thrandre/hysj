<script lang="ts">
  // What the window shows until the first snapshot arrives. A long wait is
  // usually macOS asking about the keychain (after an update): say so then.
  import { clock } from '../lib/clock.svelte.ts';

  const started = Date.now() / 1000;
  let slow = $derived(clock.now - started >= 4);
</script>

<div class="opening" role="status" aria-live="polite">
  <span class="mark" aria-hidden="true">Hysj<span class="bang">!</span></span>
  <p class="line">Opening your mail.</p>
  <p class="hint" class:shown={slow}>
    If macOS asks to use your keychain, choose <strong>Always Allow</strong> — that’s where Hysj keeps your passwords.
  </p>
</div>

<style>
  .opening {
    min-height: calc(100vh - 72px - 160px);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 14px;
    padding: 0 24px;
    text-align: center;
  }
  .mark {
    font-size: 96px;
    line-height: 1;
    font-weight: 700;
    letter-spacing: -0.06em;
  }
  /* The "!" breathes: Hysj is working, quietly. */
  .bang {
    display: inline-block;
    animation: breathe 2.4s ease-in-out infinite;
  }
  .line {
    margin: 0;
    font-size: 19px;
    color: var(--body);
  }
  .hint {
    max-width: 420px;
    margin: 0;
    font-size: 14px;
    line-height: 1.55;
    color: var(--secondary);
    opacity: 0;
    transition: opacity 0.6s ease-out;
  }
  .hint.shown {
    opacity: 1;
  }
  @keyframes breathe {
    50% {
      opacity: 0.2;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .bang {
      animation: none;
    }
    .hint {
      transition: none;
    }
  }
</style>
