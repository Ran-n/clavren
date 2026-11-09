// Audit actions in words. An action the dashboard doesn't know yet is still shown, readably.

const ACTIONS: Record<string, string> = {
  "admin.created": "Admin created",
  "admin.login": "Signed in",
  "admin.login_failed": "Failed sign-in",
  "admin.logout": "Signed out",
  "admin.sessions_revoked": "Signed out other sessions",
  "admin.password_changed": "Changed password",
  "admin.password_reset": "Password reset on the server",
  "admin.recovery_codes_regenerated": "New recovery codes",
  "admin.totp_changed": "Moved to a new authenticator app",
  "admin.disabled": "Admin disabled",
  "admin.enabled": "Admin enabled",
  "product.created": "Product created",
  "product.updated": "Product changed",
  "product.deleted": "Product deleted",
  "customer.created": "Customer created",
  "customer.updated": "Customer changed",
  "customer.deleted": "Customer deleted",
  "licence.issued": "Licence issued",
  "licence.updated": "Licence changed",
  "licence.revoked": "Licence revoked",
  "licence.reset": "Licence reset",
  "licence.deleted": "Licence deleted",
  "licence.key_revealed": "Key revealed",
  "licence.message_copied": "Message copied",
  "licence.exported": "Exported to CSV",
  "activation.created": "Computer activated",
  "check.refused": "Check refused",
  "server.update_requested": "Server update requested",
};

export function describeAction(action: string): string {
  const known = ACTIONS[action];
  if (known) return known;
  const words = action.replace(/[._]/g, " ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** The action in words, with the details that matter for it (an update's versions). */
export function describeEntry(action: string, metadata: Record<string, unknown>): string {
  const words = describeAction(action);
  if (action !== "server.update_requested") return words;
  const { from, to, skip_backup: skipBackup } = metadata;
  if (typeof from !== "string" || typeof to !== "string") return words;
  return `${words}: ${from} to ${to}${skipBackup === true ? ", without a backup" : ""}`;
}

export const ACTION_FILTERS = [
  { value: "", label: "Every action" },
  { value: "licence.", label: "Licences" },
  { value: "activation.", label: "Activations" },
  { value: "check.", label: "Refused checks" },
  { value: "customer.", label: "Customers" },
  { value: "product.", label: "Products" },
  { value: "admin.", label: "Admins and sign-ins" },
  { value: "server.", label: "Server" },
  ...Object.entries(ACTIONS).map(([value, label]) => ({ value, label: `· ${label}` })),
];
