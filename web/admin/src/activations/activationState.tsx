// An activation's state in words: in use, or why it stopped.

import { Badge } from "../shared/components/Badge";
import { formatDate } from "../shared/format";
import type { Activation } from "../shared/types";

const REASONS: Record<string, string> = {
  reset: "Reset",
  licence_revoked: "Licence revoked",
};

export function activationState(activation: Activation) {
  if (!activation.revoked_at) return <Badge tone="good">In use</Badge>;
  const reason = REASONS[activation.revocation_reason ?? ""] ?? "Ended";
  return <Badge tone="neutral">{`${reason} ${formatDate(activation.revoked_at)}`}</Badge>;
}
