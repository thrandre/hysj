// The daemon's types, generated from crates/hysj-proto by ts-rs
// (`cargo test -p hysj-proto --features ts`). Do not edit gen/ by hand.

export type * from './gen/index';

import type { Account, BookingItem, CodeItem, CodeSettings, Contact, Identity, MessageSummary, OrderItem, Signal } from './gen/index';

/** Everything the UI shows, fetched together on every change. */
export interface Snapshot {
  accounts: Account[];
  identities: Identity[];
  contacts: Contact[];
  people: MessageSummary[];
  digest: MessageSummary[];
  screener: MessageSummary[];
  sent: MessageSummary[];
  done: MessageSummary[];
  codes: CodeItem[];
  codeSettings: CodeSettings;
  bookings: BookingItem[];
  orders: OrderItem[];
  signals: Signal[];
  digestHours: number[];
}

/** The app's own update state (hysj-app's update.rs; not a daemon type). */
export interface UpdateStatus {
  current: string;
  ready: string | null;
  notes: string | null;
  auto: boolean;
  checked: number | null;
  can_update: boolean;
}
