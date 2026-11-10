// Small coloured labels for states.

import type { LicenceState } from "../types";

const LICENCE_STATES: Record<LicenceState, { label: string; tone: string }> = {
  unused: { label: "Unused", tone: "neutral" },
  redeemed: { label: "In use", tone: "good" },
  revoked: { label: "Revoked", tone: "bad" },
};

export function Badge({ tone, children }: { tone: string; children: string }) {
  return <span className={`badge badge--${tone}`}>{children}</span>;
}

export function LicenceStateBadge({ state }: { state: LicenceState }) {
  const { label, tone } = LICENCE_STATES[state];
  return <Badge tone={tone}>{label}</Badge>;
}

/** Marks a test licence. */
export function TestBadge() {
  return <Badge tone="test">Test</Badge>;
}

/** A licence's state, and whether it is a test licence. */
export function LicenceBadges({ state, test }: { state: LicenceState; test: boolean }) {
  return (
    <span className="badges">
      <LicenceStateBadge state={state} />
      {test && <TestBadge />}
    </span>
  );
}

export const LICENCE_STATE_OPTIONS = [
  { value: "", label: "Any state" },
  { value: "unused", label: "Unused" },
  { value: "redeemed", label: "In use" },
  { value: "revoked", label: "Revoked" },
];
