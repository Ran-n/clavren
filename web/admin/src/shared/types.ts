// The admin API's records, as docs/api.md describes them. A related record is its ID unless the
// request expanded it.

export type Id = string;
export type Related<T> = Id | T;

export type Page<T> = { data: T[]; next_cursor: string | null };

/** An amount in the currency's minor unit (cents for EUR) and its ISO 4217 code. */
export type Price = { amount: number; currency: string };

export type Product = {
  id: Id;
  code: string;
  name: string;
  description: string | null;
  message_template: string;
  is_active: boolean;
  /** Whether people may apply for a licence of it. */
  accepts_applications: boolean;
  /** How to get a key, shown on its key page at `/apply?product=<code>`. */
  key_instructions: string | null;
  /** How many days its applications keep working without reaching the server (2 to 3,650). */
  offline_days: number;
  /** Its price per key, filled in for each new licence. */
  price: Price | null;
  created_at: string;
  updated_at: string;
};

export type Customer = {
  id: Id;
  name: string;
  email: string | null;
  notes: string | null;
  created_at: string;
  updated_at: string;
};

export type LicenceState = "unused" | "redeemed" | "revoked";

export type Activation = {
  id: Id;
  licence: Related<Licence>;
  device_name: string;
  platform: string;
  app_version: string;
  activated_at: string;
  last_checked_at: string | null;
  revoked_at: string | null;
  revocation_reason: "reset" | "licence_revoked" | null;
};

export type Licence = {
  id: Id;
  product: Related<Product>;
  customer: Related<Customer> | null;
  state: LicenceState;
  /** The latest computer it is active on; a test licence can be on several. */
  active_activation: Related<Activation> | null;
  /** How many computers it is active on. */
  active_computers: number;
  /** A test licence: for trying an application, on any number of computers, left out of the Overview. */
  is_test: boolean;
  created_at: string;
  expires_at: string | null;
  revoked_at: string | null;
  revocation_reason: string | null;
  internal_notes: string | null;
  /** What its key was charged; none on a test licence. */
  price: Price | null;
};

export type AuditEntry = {
  id: Id;
  admin_user: Related<{ id: Id; username: string }> | null;
  action: string;
  entity_type: string;
  entity_id: Id | null;
  metadata: Record<string, unknown>;
  ip_address: string | null;
  created_at: string;
};

export type BackupRun = { id: Id; finished_at: string; archive_name: string; size_bytes: number };

export type Overview = {
  active_products: number;
  inactive_products: number;
  unused: number;
  redeemed: number;
  revoked: number;
  active_computers: number;
  computers_checked_last_24h: number;
  checks_last_24h: number;
  last_backup: BackupRun | null;
  backup_overdue: boolean;
  /** Licence applications waiting for a decision. */
  pending_applications: number;
  /** What keys were charged, per currency, in minor units. */
  charged: { currency: string; last_30_days: number; total: number }[];
  /** Licences (not test ones) with no recorded price. */
  unpriced: number;
  /** The currency the dashboard preselects for prices. */
  default_currency: string;
};

/** The record when it was expanded, otherwise null. */
export function expanded<T extends { id: Id }>(value: Related<T> | null | undefined): T | null {
  return value !== null && typeof value === "object" ? value : null;
}

export function idOf<T extends { id: Id }>(value: Related<T> | null | undefined): Id | null {
  if (value === null || value === undefined) return null;
  return typeof value === "string" ? value : value.id;
}

export type UpdateStage = "starting" | "downloading" | "checking" | "backing_up" | "installing" | "restarting";

/** How the current or last update to a newer version went. */
export type UpdateStatus = {
  version: string | null;
  state: "running" | "succeeded" | "failed";
  stage: UpdateStage | null;
  message: string | null;
  started_at: string;
  finished_at: string | null;
};

/** `GET /updates`: the running version, the last check, a newer release and the update status. */
export type Updates = {
  running_version: string;
  checking: "on" | "off" | "never_checked";
  last_check: { checked_at: string; error: string | null } | null;
  available: { version: string; release_url: string | null; published_at: string | null; installable: boolean } | null;
  backup_drive_mounted: boolean;
  update: UpdateStatus | null;
  updater_installed: boolean;
  can_install: boolean;
};

/** The signed-in admin's session, from signing in and from `GET /session`. */
export type Session = { admin: { id: Id; username: string }; csrf_token: string; expires_at: string };
