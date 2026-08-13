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
  const cacheRef = React.useRef<WorkbenchObserverBundle[]>([]);
  const getSnapshot = React.useCallback(() => {
    const next = keys.map((agentPubkey) => ({
      agentPubkey,
      items: getAgentTranscript(agentPubkey, true),
    }));
    const prev = cacheRef.current;
    if (
      prev.length === next.length &&
      prev.every(
        (bundle, index) =>
          bundle.agentPubkey === next[index]?.agentPubkey &&
          bundle.items === next[index]?.items,
      )
    ) {
      return prev;
    }
    cacheRef.current = next;
    return next;
  }, [keys]);
  const snapshot = React.useSyncExternalStore(
    subscribeAgentObserverStore,
    getSnapshot,
    getSnapshot,
  );
  return useStableArrayShallow(snapshot);
}
