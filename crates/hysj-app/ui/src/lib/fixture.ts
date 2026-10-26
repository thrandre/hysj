// Synthetic mail for working on the UI in a browser (the mockups' cast).
// Times are relative to now, so waits and countdowns look alive.

import type { Edition, MessageSummary, Snapshot } from './types.ts';

export async function fixture(): Promise<Snapshot> {
  const now = Math.floor(Date.now() / 1000);
  const h = 3600;
  let n = 0;
  const msg = (m: Partial<MessageSummary> & Pick<MessageSummary, 'from' | 'subject'>): MessageSummary => ({
    id: String(++n),
    account: 'work',
    from_addr: `${m.from.toLowerCase().replace(/[^a-z]+/g, '.')}@example.com`,
    date: null,
    ts: now - 2 * h,
    bucket: 'people',
    reason: { kind: 'emailed_before', times: 3 },
    intent: 'reply',
    replied: false,
    arrived_on: 'me@work.example',
    tags: [],
    replies: [],
    ...m,
  });
  const people = [
    msg({ from: 'Aisha Rahman', subject: 'Re: Contract v3', gist: 'Contract v3 is ready — OK to send it for signing?', ts: now - 50 * h, replies: ['Looks good — send it', 'Can we talk about §4 first?'] }),
    msg({ from: 'Ingrid Solheim', account: 'personal', arrived_on: 'me@gmail.example', subject: 'Styremøte 28. oktober', gist: 'Can you present the budget item at the board meeting on the 28th?', ts: now - 5 * h, replies: ['Yes, I’ll take it', 'Can’t that day'] }),
    msg({ from: 'Jonas Lie', account: 'personal', arrived_on: 'me@gmail.example', subject: 'Klatring torsdag?', gist: 'Climbing Thursday at 18:00? He can pick you up.', ts: now - 6 * h }),
    msg({ from: 'Marcus Berg', subject: 'Re: Sync design', gist: 'Last-writer-wins for message bodies — pair on it Thursday?', ts: now - 1 * h }),
    msg({ from: 'Elena Novak', subject: 'Intro: Priya', intent: 'read', gist: 'Introduced you to Priya, who’s building local-first sync.', ts: now - 20 * h }),
    msg({ from: 'Pappa', account: 'personal', subject: 'Bilder fra hytta', intent: 'know', gist: 'Sent 12 photos from the cabin.', ts: now - 22 * h }),
  ];
  const digest = [
    msg({ from: 'Rust weekly roundup', subject: 'Async closures land on stable', bucket: 'digest', intent: 'read', reason: { kind: 'bulk', signal: 'List-Unsubscribe' }, gist: 'The headline feature of the next release, plus a writeup on compile times.', ts: now - 9 * h }),
    msg({ from: 'Local-first newsletter', subject: 'Sync without a server', bucket: 'digest', intent: 'read', reason: { kind: 'bulk', signal: 'List-Unsubscribe' }, ts: now - 10 * h }),
    msg({ from: 'GitHub', subject: '[hysj] CI passed on main', bucket: 'digest', intent: 'know', reason: { kind: 'no_reply' }, ts: now - 11 * h }),
    msg({ from: 'GitHub', subject: '[hysj] Review requested: Batch IMAP IDLE reconnects', bucket: 'digest', intent: 'know', reason: { kind: 'no_reply' }, ts: now - 12 * h }),
  ];
  const screener = [
    msg({ from: 'Tor Haugen', subject: 'Snow clearing contract for the building', bucket: 'screener', reason: { kind: 'first_time' }, ts: now - 3 * h }),
    msg({ from: 'Recruiting team', subject: 'Senior Rust role — quick chat?', bucket: 'screener', reason: { kind: 'delivery_service', service: 'SendGrid' }, ts: now - 4 * h }),
  ];
  return {
    accounts: [
      { name: 'personal', address: 'me@gmail.example', host: 'imap.gmail.com', port: 993, username: 'me@gmail.example', smtp_host: 'smtp.gmail.com', smtp_port: 465, label: 'Personal' },
      { name: 'work', address: 'me@work.example', host: 'imap.work.example', port: 993, username: 'me@work.example', smtp_host: 'smtp.work.example', smtp_port: 465, label: 'Work' },
    ],
    identities: [
      { account: 'personal', address: 'me@gmail.example', name: 'Kai' },
      { account: 'work', address: 'me@work.example', name: 'Kai' },
      { account: 'work', address: 'post@work.example', name: null },
    ],
    contacts: [
      { address: 'marcus.berg@example.com', name: 'Marcus Berg', times: 12 },
      { address: 'aisha.rahman@example.com', name: 'Aisha Rahman', times: 9 },
    ],
    people,
    digest,
    screener,
    sent: [msg({ from: 'Kai', subject: 'Re: Contract v2', to: 'Aisha Rahman', bucket: 'people', ts: now - 70 * h })],
    done: [
      msg({ from: 'Aisha Rahman', subject: 'Re: Contract v2', replied: true, ts: now - 26 * h }),
      msg({ from: 'Spond', subject: 'Training moved to 19:00', bucket: 'digest', intent: 'know', reason: { kind: 'no_reply' }, ts: now - 30 * h }),
    ],
    codes: [
      { id: 'c1', kind: 'sign-in code', reset: false, service: 'Example', sender_domain: 'example.com', code: '482913', link: null, link_host: null, link_ok: false, arrived: now - 60, expires: now + 540, reported: false },
    ],
    codeSettings: { popup: true, retention_hours: 24 },
    bookings: [
      { key: 'b1', kind: 'flight', reference: 'K7P2QX', title: 'Oslo → Bergen', start: '2026-11-06T07:30:00', end: '2026-11-06T08:25:00', start_ts: now + 17 * 24 * h, provider: 'Airline', city: 'Bergen', seat: '12A', party: null, moved_from: '2026-11-06T07:15:00', sources: 2, message: null, pass: { id: 'm1', index: 0 } },
      { key: 'b2', kind: 'hotel', reference: '48210937', title: 'Hotel, Bryggen', start: '2026-11-06T15:00:00', end: '2026-11-08T11:00:00', start_ts: now + 17 * 24 * h + 8 * h, provider: null, city: 'Bergen', seat: null, party: null, moved_from: null, sources: 1, message: null, pass: null },
    ],
    digestHours: [8, 16],
    signals: [
      { id: 's1', ids: ['s1'], kind: 'payment', from: 'Example Cloud', title: '$20.00 payment to Example Cloud was unsuccessful again', ts: now - 2 * h },
      { id: 's2', ids: ['s2', 's3'], kind: 'security', from: 'Google', title: 'Security alert for your account', ts: now - 5 * h },
    ],
    orders: [
      { key: 'o1', store: 'Electronics store', number: '1001', ordered: '2026-10-19', ordered_ts: now - 2 * 24 * h, items: ['USB-C dock'], total: 'NOK 1 290', status: 3, eta: null, delivered_ts: null, tracking_url: 'https://example.com/track', return_by: null, sources: 2, receipt: null },
    ],
  };
}

/** A synthetic edition shaped like the Digest mockup. */
export function fixtureEdition(): Edition {
  const it = (id: string, title: string, sub: string, meta = '', extra: Partial<Edition['stories'][0]['sections'][0]['items'][0]> = {}) => ({
    id,
    title,
    sub,
    meta,
    ...extra,
  });
  return {
    pick: 0,
    meta: 'Tuesday · 16:00 digest · since 08:00 · 4 min',
    controls: [
      { label: 'Pause cabin alerts', key: 'finn_pause:cabin', on: true },
      { label: 'Only tell me about price drops', key: 'finn_drops_only', on: true },
    ],
    unsure: [
      { id: 'u1', from: 'Helsenorge', sender: 'noreply@helsenorge.no', title: 'Ny melding på Helsenorge', why: 'Held because it looks like a notification — but it’s from a public service' },
      { id: 'u2', from: 'Vigilo', sender: 'donotreply@vigilo.no', title: 'Ny melding i Vigilo', why: 'Held because it looks like a notification — but it’s from a school or kindergarten' },
    ],
    headline: 'Nothing here needs you. FINN has new listings for you.',
    lede: 'Forty-one emails came in since 08:00. Three newsletters are worth reading, Spond sent four updates and twelve promotions went straight to the archive.',
    stories: [
      {
        kind: 'listings',
        label: 'Listings',
        head: 'FINN found you 4 jobs, 3 homes and 10 cabins.',
        para: 'The house in Ski has dropped in price since you last saw it.',
        sections: [
          {
            title: 'Homes',
            layout: 'cards',
            shown: 3,
            items: [
              it('h1', 'Enebolig med utsikt', 'Ski · 5 rom · 168 m²', 'kr 6 450 000', { tag: 'Price dropped' }),
              it('h2', 'Leilighet med balkong', 'Grünerløkka · 3 rom · 74 m²', 'kr 5 900 000', { tag: 'Viewing Sunday' }),
              it('h3', 'Rekkehus med hage', 'Nordstrand · 4 rom · 112 m²', 'kr 8 100 000', { tag: 'New' }),
            ],
          },
          {
            title: 'Jobs',
            layout: 'list',
            shown: 3,
            items: [
              it('j1', 'Senior Rust-utvikler', 'Example AS · Oslo · hybrid', 'Frist 1. nov'),
              it('j2', 'Platform Engineer', 'Example AS · Oslo · remote', 'Frist 8. nov'),
              it('j3', 'Tech Lead, betaling', 'Example AS · Bergen', 'Snarest'),
              it('j4', 'Systemutvikler (Rust/Go)', 'Example AS · Trondheim', 'Frist 15. nov'),
            ],
          },
          {
            title: 'Cabins',
            layout: 'compact',
            shown: 2,
            items: [
              it('c1', 'Fjellhytte, Hemsedal', '4 soverom · ski inn/ut', 'kr 3 900 000'),
              it('c2', 'Familiehytte, Geilo', '5 soverom · badstue', 'kr 4 400 000'),
              it('c3', 'Rorbu, Lofoten', '2 soverom · sjøkant', 'kr 2 100 000'),
            ],
          },
        ],
      },
      {
        kind: 'services',
        label: 'Accounts & services',
        head: 'Twenty-nine notifications, and only one wants something from you.',
        para: '',
        sections: [{ layout: 'lines', shown: 3, items: [it('s1', 'Podme', 'Abonnement kunne ikke fornyes', '2 emails', { tag: 'Needs you' }), it('s2', 'Google', 'Security alert', '21 Oct')] }],
      },
      {
        kind: 'reading',
        label: 'Reading',
        head: 'Three newsletters worth a look.',
        para: '',
        sections: [
          {
            layout: 'reading',
            shown: 3,
            items: [
              it('r1', 'Async closures land on stable', 'Rust weekly roundup · 21 Oct', '', { text: 'The headline feature of the next release, plus a writeup on compile times.' }),
              it('r2', 'Sync without a server', 'Local-first newsletter · 21 Oct', ''),
            ],
          },
        ],
      },
      { kind: 'money', label: 'Money', head: 'Six receipts and bookings, all filed.', para: 'Bookings live in Bookings and orders in Orders — shipping updates move them along without reaching you.', sections: [] },
    ],
    ending: {
      text: 'That’s everything since 08:00. Next digest tomorrow at 08:00.',
      note: 'Twelve promotions from nine shops went straight to the archive.',
      promotions: [it('p1', 'Jollyroom', 'WOW-priser på vinterklær', '3 emails'), it('p2', 'Superkul', '40% rabatt på kostymer', '21 Oct')],
    },
    ids: ['h1', 'h2', 'h3'],
  };
}
