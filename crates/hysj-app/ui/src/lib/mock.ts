// The browser-only Rust side: answers from `public/snapshot.local.json` (a dump
// of real mail, never committed) or the synthetic fixture.

import type { Snapshot } from './types.ts';
import { fixture, fixtureEdition } from './fixture.ts';

let data: Promise<Snapshot> | null = null;

function load(): Promise<Snapshot> {
  data ??= fetch('/snapshot.local.json')
    .then((r) => (r.ok ? (r.json() as Promise<Snapshot>) : fixture()))
    .catch(() => fixture());
  return data;
}

export async function mock(name: string, args: Record<string, unknown>): Promise<unknown> {
  const s = await load();
  const answers: Record<string, () => unknown> = {
    account_list: () => s.accounts,
    // The preview's stand-in for the daemon's provider.rs.
    account_discover: () => {
      const d = String(args.address).split('@')[1]?.toLowerCase() ?? '';
      const name = d.split('.')[0];
      const ms = ['hotmail', 'outlook', 'live', 'msn'].includes(name);
      const gmail = d === 'gmail.com';
      return {
        id: ms ? 'microsoft' : gmail ? 'gmail' : 'generic',
        name: ms ? 'Outlook.com' : gmail ? 'Gmail' : d,
        known: ms || gmail,
        auth: ms || gmail ? 'oauth' : 'password',
        imap_host: ms ? 'outlook.office365.com' : gmail ? 'imap.gmail.com' : `imap.${d}`,
        imap_port: 993,
        smtp_host: ms ? 'smtp-mail.outlook.com' : gmail ? 'smtp.gmail.com' : `smtp.${d}`,
        smtp_port: ms ? 587 : 465,
        help_url: null,
        help_label: null,
        help_steps: gmail
          ? ['Google will say it hasn’t verified Hysj — it’s a small app Google hasn’t reviewed. Choose Advanced, then Go to Hysj.', 'Then allow Hysj to read, send and manage your mail.']
          : [],
        sign_in_with: ms ? 'Microsoft' : gmail ? 'Google' : null,
      };
    },
    account_label: () => {
      const a = s.accounts.find((x) => x.name === args.name);
      if (a) a.label = (args.label as string | null)?.trim() || null;
      return null;
    },
    identity_list: () => s.identities,
    contact_list: () => s.contacts,
    list: () => (args.bucket === 'blocked' ? s.screener.slice(0, 1).map((m) => ({ ...m, id: 'blocked-1', bucket: 'blocked', reason: { kind: 'decision', decision: 'block' } })) : s[args.bucket as 'people' | 'digest' | 'screener']) ?? [],
    folder_list: () => s[args.folder as 'sent' | 'done'] ?? [],
    code_list: () => s.codes,
    code_settings: () => s.codeSettings,
    booking_list: () => s.bookings,
    order_list: () => s.orders,
    kind_correct: () => {
      s.bookings = s.bookings.filter((b) => b.key !== args.record);
      s.orders = s.orders.filter((o) => o.key !== args.record);
      return null;
    },
    signal_list: () => s.signals ?? [],
    ai_settings: () => ({ enabled: true, on_battery: false, waiting_for_power: false, model: 'gemma3-4b-q4k', size: 'apple', recommended: 'apple', apple_available: true, apple_reason: '', ...(args.set as object | null) }),
    app_version: () => '0.1.0',
    system_info: () => 'macOS 26.0 · Mac16,1 · Apple M4 · 24 GB',
    // ?update in the preview shows a downloaded update.
    update_status: () => ({
      current: '0.1.0',
      ready: new URLSearchParams(location.search).has('update') ? '0.2.0' : null,
      notes: 'Notifications that can’t wait now reach you before the digest, and bookings get an Add to Wallet button.',
      auto: true,
      checked: Math.floor(Date.now() / 1000) - 3600,
      can_update: true,
    }),
    update_check: () => answers.update_status(),
    update_auto: () => answers.update_status(),
    digest_hours: () => {
      if (Array.isArray(args.hours)) s.digestHours = args.hours as number[];
      return s.digestHours ?? [8, 16];
    },
    message_view: () => mockView(String(args.id)),
    search: () => [],
    // A real dump may carry editions (pick 0–3); else the synthetic one.
    digest_edition: () => (s as Snapshot & { editions?: unknown[] }).editions?.[Number(args.pick)] ?? fixtureEdition(),
  };
  const answer = answers[name];
  if (!answer) {
    console.info('[mock]', name, args);
    return null;
  }
  return answer();
}

/** A rendered message for the browser: an HTML reply (quote, a link that lies,
 * a remote image, an attachment) or a plain-text forward. */
function mockView(id: string) {
  if (id === '1')
    return {
      id,
      plain: false,
      html: `<div dir="ltr"><p>Hi Kai,</p><p>Only two changes this round: the notice period in §4 and the IP clause wording.
<b>OK to send for signing?</b></p><p>The draft is on <a href="https://docs.example.com/contract-v3">docs.example.com</a>,
and the bank details are at <a href="https://login.example.ru/dnb">www.dnb.no</a>.</p>
<p><img src="https://cdn.example.com/signature-logo.png" alt="Logo" width="120" height="40"></p>
<div class="gmail_signature">Aisha Rahman · Legal · +47 000 00 000</div>
<div class="gmail_quote"><div class="gmail_attr">On Fri, 16 Oct 2026, Kai wrote:</div>
<blockquote class="gmail_quote">Thanks! Just the notice period left on my side.</blockquote></div></div>`,
      attachments: [{ index: 0, name: 'Contract v3.pdf', mime: 'application/pdf', size: 248_000 }],
      remote_images: 1,
      images_allowed: false,
      sender: 'aisha@example.com',
    };
  return {
    id,
    plain: true,
    html:
      '<div class="hysj-plain">Se under.\n\n---------- Forwarded message ---------\nFra: Siri &lt;siri@example.no&gt;\nSubject: Innkalling\n\nVelkommen til styremøte 28. oktober kl. 19:30. Sakliste: <a href="https://example.no/sakliste">https://example.no/sakliste</a></div>' +
      '<div class="hysj-plain hysj-quoted">--\nMvh Ingrid</div>',
    attachments: [],
    remote_images: 0,
    images_allowed: false,
    sender: 'ingrid@example.no',
  };
}
