import * as React from "react";

import { useKeepMentionedAgentsPinned } from "@/features/messages/lib/autoPinMentionedAgentsPreference";
import {
  initializePersistentAgentAudience,
  usePersistentAgentAudience,
} from "@/features/messages/lib/persistentAgentAudience";

export function useThreadAgentAudience({
  isAgentPubkey,
  initialAgentPubkeys,
  rootTags,
  scope,
}: {
  isAgentPubkey: (pubkey: string) => boolean;
  initialAgentPubkeys?: readonly string[];
  rootTags: readonly string[][];
  scope: string | null;
}) {
  const audience = usePersistentAgentAudience(scope);
  const keepMentionedAgentsPinned = useKeepMentionedAgentsPinned();

  const rootAgentPubkeys = React.useMemo(
    () =>
      [
        ...(initialAgentPubkeys ?? []),
        ...rootTags.flatMap((tag) =>
          tag[0] === "p" && tag[1] ? [tag[1]] : [],
        ),
      ].filter(isAgentPubkey),
    [initialAgentPubkeys, isAgentPubkey, rootTags],
  );

  React.useEffect(() => {
    if (!scope || !keepMentionedAgentsPinned) return;
    initializePersistentAgentAudience(scope, rootAgentPubkeys);
  }, [keepMentionedAgentsPinned, rootAgentPubkeys, scope]);

  return { audience, keepMentionedAgentsPinned };
}
