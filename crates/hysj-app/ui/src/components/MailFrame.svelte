<script lang="ts">
  // One mail, rendered. SECURITY — this frame is the trust boundary:
  //  - sandbox="allow-same-origin" and NEVER allow-scripts. Same-origin without
  //    scripts lets the app measure, fold and catch clicks in the frame, while
  //    nothing in the mail can run. Both together would let a mail's script
  //    reach the app's commands (send, …).
  //  - The frame's own CSP blocks every remote load (images, CSS url(), fonts)
  //    until the reader allows images; the HTML was sanitised by the daemon.
  //  - Links never navigate the frame: clicks are caught here, and only
  //    http(s) opens (in the browser, from Rust); mailto: opens Compose.
  import { api } from '../lib/api.ts';
  import { fold } from '../lib/fold.ts';
  import type { MessageView } from '../lib/types.ts';
  import { linkHost, linkWarning } from '../lib/words.ts';

  let {
    view,
    showFolded = false,
    onfolded = (_n: number) => {},
    onmailto = (_to: string) => {},
  }: {
    view: MessageView;
    showFolded?: boolean;
    onfolded?: (n: number) => void;
    onmailto?: (to: string) => void;
  } = $props();

  let frame: HTMLIFrameElement | undefined = $state();
  /** Images shown for this mail only, or (with `always`) for the sender from now on. */
  // Keyed by message, so Focus moving on doesn't carry it to the next mail.
  let showImagesFor = $state<string | null>(null);
  let images = $derived(view.images_allowed || showImagesFor === view.id);

  async function always() {
    await api.senderImages(view.sender, true);
    showImagesFor = view.id;
  }
  let height = $state(80);
  /** Where the hovered link goes (the status line under the mail). */
  let hover = $state<{ host: string; warning: string | null } | null>(null);

  // The app's colours and font, for plain-text mail (the frame can't see our CSS variables).
  const css = (name: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();

  let srcdoc = $derived.by(() => {
    const img = images ? 'data: https:' : 'data:';
    const csp = `default-src 'none'; style-src 'unsafe-inline'; img-src ${img}; font-src 'self'`;
    const plain = view.plain
      ? `:root{color-scheme:${getComputedStyle(document.documentElement).colorScheme || 'light'}}
         body{font:16px/1.55 'Instrument Sans',system-ui,sans-serif;color:${css('--body')};background:transparent}
         .hysj-plain{white-space:pre-wrap;overflow-wrap:anywhere}
         a{color:${css('--ink')}}`
      : `:root{color-scheme:light}body{font:15px/1.5 -apple-system,system-ui,sans-serif;color:#111;background:#fff}`;
    return `<!doctype html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="${csp}">
<style>
@font-face{font-family:'Instrument Sans';src:url('/fonts/InstrumentSans.ttf');font-weight:400 700}
html,body{margin:0;padding:0}
body{padding:${view.plain ? '0 2px' : '20px'};overflow-wrap:break-word}
${plain}
img{max-width:100%;height:auto}
table{max-width:100%}
.hysj-folded{display:none!important}
body.show-folded .hysj-folded{display:revert!important;opacity:.75}
a.hysj-warn{outline:2px solid #b42318;outline-offset:2px}
</style></head><body class="${view.plain ? 'plain' : 'html'}">${view.html}</body></html>`;
  });

  function measure() {
    const doc = frame?.contentDocument;
    if (doc?.documentElement) height = Math.max(40, doc.documentElement.scrollHeight);
  }

  function loaded() {
    const doc = frame?.contentDocument;
    if (!doc) return;
    onfolded(fold(doc));
    doc.body.classList.toggle('show-folded', showFolded);
    // Mark links whose text names another site than they go to.
    for (const a of Array.from(doc.querySelectorAll('a[href]'))) {
      const w = linkWarning(a.textContent ?? '', a.getAttribute('href') ?? '');
      if (w) a.classList.add('hysj-warn');
    }
    doc.addEventListener('click', (e) => {
      const a = (e.target as Element | null)?.closest('a');
      if (!a) return;
      e.preventDefault();
      const href = (a.getAttribute('href') ?? '').trim();
      if (/^mailto:/i.test(href)) onmailto(decodeURIComponent(href.slice(7).split('?')[0]));
      else if (/^https?:\/\//i.test(href)) void api.openLink(href);
    });
    doc.addEventListener('mouseover', (e) => {
      const a = (e.target as Element | null)?.closest('a');
      const href = a?.getAttribute('href') ?? '';
      hover = a && linkHost(href) ? { host: linkHost(href), warning: linkWarning(a.textContent ?? '', href) } : null;
    });
    doc.addEventListener('mouseleave', () => (hover = null));
    measure();
    void doc.fonts.ready.then(measure);
    new ResizeObserver(measure).observe(doc.body);
    // Images arrive after load.
    for (const img of Array.from(doc.images)) img.addEventListener('load', measure);
  }

  $effect(() => {
    frame?.contentDocument?.body?.classList.toggle('show-folded', showFolded);
    measure();
  });
</script>

{#if view.remote_images > 0 && !images}
  <p class="note">
    {view.remote_images} image{view.remote_images === 1 ? '' : 's'} not loaded (they can tell the sender you read this).
    <button class="link" onclick={() => (showImagesFor = view.id)}>Show images</button> ·
    <button class="link" onclick={always}>Always for {view.sender}</button>
  </p>
{/if}
<div class="mail" class:paper={!view.plain}>
  <iframe
    bind:this={frame}
    title="Message"
    sandbox="allow-same-origin"
    referrerpolicy="no-referrer"
    {srcdoc}
    style:height="{height}px"
    onload={loaded}
  ></iframe>
</div>
<p class="status" aria-live="polite">
  {#if hover}
    {#if hover.warning}<span class="warn">{hover.warning}</span>{:else}Opens {hover.host}{/if}
  {/if}
</p>

<style>
  .mail {
    border-radius: 18px;
    overflow: hidden;
  }
  /* HTML mail is designed for white; it stays on white "paper" in dark mode. */
  .mail.paper {
    background: #fff;
    box-shadow: var(--shadow-sm);
  }
  iframe {
    display: block;
    width: 100%;
    border: 0;
    background: transparent;
  }
  .note {
    margin: 0 0 10px;
    color: var(--muted);
    font-size: 13px;
  }
  .link {
    border: 0;
    background: none;
    padding: 4px 2px;
    color: var(--ink);
    text-decoration: underline;
    font-size: 13px;
  }
  .status {
    min-height: 18px;
    margin: 6px 2px 0;
    font-size: 12px;
    color: var(--muted);
  }
  .warn {
    color: var(--danger);
    font-weight: 600;
  }
</style>
