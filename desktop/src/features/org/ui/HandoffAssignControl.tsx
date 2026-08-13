import * as React from "react";

import { useOrgRosterQuery } from "@/features/org/hooks/useOrgRosterQuery";
import {
  authorMayAssign,
  displayNameForPubkey,
} from "@/features/org/lib/orgRoster";
import { setPendingHandoffExecutor } from "@/features/org/lib/pendingHandoff";
import { useIdentityQuery } from "@/shared/api/hooks";
import { useUsersBatchQuery } from "@/features/profile/hooks";
import { truncatePubkey } from "@/shared/lib/pubkey";

export function HandoffAssignControl() {
  const identity = useIdentityQuery();
  const rosterQuery = useOrgRosterQuery();
  const [executor, setExecutor] = React.useState("");
  const roster = rosterQuery.data;
  const author = identity.data?.pubkey ?? "";
  const candidates = React.useMemo(() => {
    if (!roster || !author) {
      return [];
    }
    return Object.keys(roster.nodes).filter((pubkey) =>
      authorMayAssign(roster, author, pubkey),
    );
  }, [author, roster]);
  const profilesQuery = useUsersBatchQuery(candidates);

  const requiresParent = Boolean(
    author && roster && author.toLowerCase() !== roster.founderPubkey,
  );

  React.useEffect(() => {
    setPendingHandoffExecutor(executor || null, requiresParent);
    return () => setPendingHandoffExecutor(null);
  }, [executor, requiresParent]);

  if (candidates.length === 0) {
    return null;
  }

  return (
    <label className="flex items-center gap-1 text-2xs text-muted-foreground">
      Assign
      <select
        className="h-7 max-w-36 rounded-md border border-input/40 bg-background px-1 text-xs"
        data-testid="handoff-assign"
        onChange={(event) => setExecutor(event.target.value)}
        value={executor}
      >
        <option value="">none</option>
        {candidates.map((pubkey) => (
          <option key={pubkey} value={pubkey}>
            {displayNameForPubkey(pubkey, profilesQuery.data?.profiles ?? {}) ||
              truncatePubkey(pubkey)}
          </option>
        ))}
      </select>
    </label>
  );
}
