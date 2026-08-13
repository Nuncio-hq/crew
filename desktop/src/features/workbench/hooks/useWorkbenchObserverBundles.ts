import * as React from "react";

import {
  getAgentTranscript,
  subscribeAgentObserverStore,
} from "@/features/agents/observerRelayStore";
import { useStableArrayShallow } from "@/shared/hooks/useStableReference";
import { normalizePubkey } from "@/shared/lib/pubkey";
import type { WorkbenchObserverBundle } from "../lib/workbenchTranscript";

export function useWorkbenchObserverBundles(
  agentPubkeys: readonly string[],
): WorkbenchObserverBundle[] {
  const keys = React.useMemo(
    () =>
      [
        ...new Set(agentPubkeys.map((pubkey) => normalizePubkey(pubkey))),
      ].filter(Boolean),
    [agentPubkeys],
  );
  const getSnapshot = React.useCallback(
    () =>
      keys.map((agentPubkey) => ({
        agentPubkey,
        items: getAgentTranscript(agentPubkey, true),
      })),
    [keys],
  );
  const snapshot = React.useSyncExternalStore(
    subscribeAgentObserverStore,
    getSnapshot,
    getSnapshot,
  );
  return useStableArrayShallow(snapshot);
}
