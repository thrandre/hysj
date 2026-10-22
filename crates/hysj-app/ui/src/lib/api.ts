// Typed calls into the app's Rust side (crates/hysj-app/src/main.rs), one per
// command. In a plain browser (`npm run dev`) a mock answers instead, so the UI
// can be worked on and screenshotted without mail.

import type {
  Account,
  AccountAddParams,
  AiSettings,
  BookingItem,
  Bucket,
  CodeItem,
  CodeSettings,
  Contact,
  Decision,
  Edition,
  Folder,
  Found,
  Identity,
  IdentitySuggestion,
  Message,
  MessageSummary,
  MessageView,
  OrderItem,
  ScreenerDecided,
  SendParams,
  SendResult,
  Signal,
  Snapshot,
  SyncResult,
  UpdateStatus,
} from './types.ts';

const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

async function call<T>(name: string, args: Record<string, unknown> = {}): Promise<T> {
  if (!inTauri) return (await import('./mock.ts')).mock(name, args) as Promise<T>;
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(name, args);
}

export const api = {
  accountList: () => call<Account[]>('account_list'),
  identityList: () => call<Identity[]>('identity_list'),
  contactList: () => call<Contact[]>('contact_list'),
  list: (bucket: Bucket) => call<MessageSummary[]>('list', { bucket }),
  folderList: (folder: Folder) => call<MessageSummary[]>('folder_list', { folder }),
  show: (id: string) => call<Message>('show', { id }),
  codeList: () => call<CodeItem[]>('code_list'),
  codeSettings: (set?: CodeSettings) => call<CodeSettings>('code_settings', { set: set ?? null }),
  bookingList: () => call<BookingItem[]>('booking_list'),
  orderList: () => call<OrderItem[]>('order_list'),
  signalList: () => call<Signal[]>('signal_list'),
  signalHold: (kind: string, hold: boolean) => call<null>('signal_hold', { kind, hold }),
  identitySuggest: (to: string) => call<IdentitySuggestion>('identity_suggest', { to }),
  screenerDecide: (sender: string, decision: Decision) => call<ScreenerDecided>('screener_decide', { sender, decision }),
  screenerForget: (sender: string) => call<number>('screener_forget', { sender }),
  done: (ids: string[]) => call<number>('message_done', { ids }),
  restore: (ids: string[]) => call<number>('message_restore', { ids }),
  codeReport: (id: string) => call<null>('code_report', { id }),
  send: (params: SendParams) => call<SendResult>('send', { params }),
  sync: () => call<SyncResult[]>('sync'),
  accountAdd: (params: AccountAddParams) => call<Account>('account_add', { params }),
  accountLabel: (name: string, label: string | null) => call<null>('account_label', { name, label }),
  openLink: (url: string) => call<null>('open_link', { url }),
  search: (query: string) => call<Found[]>('search', { query }),
  digestEdition: (pick: number) => call<Edition>('digest_edition', { pick }),
  digestSet: (key: string, on: boolean) => call<null>('digest_set', { key, on }),
  digestHours: (hours?: number[]) => call<number[]>('digest_hours', { hours: hours ?? null }),
  aiSettings: (set?: AiSettings) => call<AiSettings>('ai_settings', { set: set ?? null }),
  appVersion: () => call<string>('app_version'),
  systemInfo: () => call<string>('system_info'),
  updateStatus: () => call<UpdateStatus>('update_status'),
  updateCheck: () => call<UpdateStatus>('update_check'),
  updateAuto: (on: boolean) => call<UpdateStatus>('update_auto', { on }),
  updateRestart: () => call<null>('update_restart'),
  messageView: (id: string) => call<MessageView>('message_view', { id }),
  senderImages: (sender: string, allow: boolean) => call<null>('sender_images', { sender, allow }),
  /** Saves to Downloads; opens it if that's safe, else shows it in Finder. Returns the path. */
  attachmentSave: (id: string, index: number, open: boolean) => call<string>('attachment_save', { id, index, open }),
};

/** Calls `f` whenever the daemon reports a change. */
export async function onChanged(f: () => void): Promise<void> {
  if (!inTauri) return;
  const { listen } = await import('@tauri-apps/api/event');
  await listen('changed', f);
}

/** Calls `f` when an update has been downloaded and is ready to install. */
export async function onUpdateReady(f: () => void): Promise<void> {
  if (!inTauri) return;
  const { listen } = await import('@tauri-apps/api/event');
  await listen('update-ready', f);
}

export async function snapshot(): Promise<Snapshot> {
  const [accounts, identities, contacts, people, digest, screener, sent, done, codes, codeSettings, bookings, orders, signals, digestHours] =
    await Promise.all([
      api.accountList(),
      api.identityList(),
      api.contactList(),
      api.list('people'),
      api.list('digest'),
      api.list('screener'),
      api.folderList('sent'),
      api.folderList('done'),
      api.codeList(),
      api.codeSettings(),
      api.bookingList(),
      api.orderList(),
      api.signalList(),
      api.digestHours(),
    ]);
  return { accounts, identities, contacts, people, digest, screener, sent, done, codes, codeSettings, bookings, orders, signals, digestHours };
}
