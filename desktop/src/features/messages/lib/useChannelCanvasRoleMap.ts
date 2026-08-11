import * as React from "react";

import { useCanvasQuery } from "@/features/channels/hooks";
import { normalizePubkey } from "@/shared/lib/pubkey";

export function useChannelCanvasRoleMap(channelId: string | null) {
  const canvasQuery = useCanvasQuery(channelId, channelId !== null);
  return React.useMemo(
    () =>
      new Map(
        (canvasQuery.data?.assignments ?? []).map((assignment) => [
          normalizePubkey(assignment.agentPubkey),
          assignment.roleLabel,
        ]),
      ),
    [canvasQuery.data?.assignments],
  );
}
