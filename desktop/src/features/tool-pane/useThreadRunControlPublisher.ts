import * as React from "react";
import { useIdentityQuery } from "@/shared/api/hooks";
import { useCommunities } from "@/features/communities/useCommunities";
import { useCurrentOwnedAgentPubkeys } from "@/features/home/useOwnedAgentPubkeys";
import {
  captureOwnerOperationScope,
  type OwnerOperationScope,
} from "@/shared/api/ownerOperations";
import { invokeTauri } from "@/shared/api/tauri";
import { normalizeRelayUrl } from "@/shared/lib/normalizeRelayUrl";
import type { ThreadRunSelection } from "./ThreadSelectedRunControls";

/** Result of one native scoped observer control publication. */
export type ThreadRunPublication = {
  status: "accepted" | "unknown" | "not_attempted";
  message?: string;
};

/** The run a control is currently bound to, with the scope it was captured under. */
export type ThreadRunCommitment = {
  run: ThreadRunSelection;
  token: OwnerOperationScope;
} | null;

/**
 * Stable identity of one live run within its conversation.
 *
 * Includes the agent: `liveRunsForThread` allows two different agents to
 * report the same session and turn id, and a key that omitted the agent
 * would collapse those into one identity.
 */
export function threadRunKey(run: {
  agentPubkey: string;
  sessionId: string;
  turnId: string;
}): string {
  return JSON.stringify([run.agentPubkey, run.sessionId, run.turnId]);
}

/**
 * Owner-scope capture and scoped publication for one thread agent's runs.
 *
 * Every surface that offers Steer/Stop for a selected run shares this seam, so
 * a second surface cannot drift into publishing with a different scope, a
 * different payload shape, or a weaker replay guard.
 */
export function useThreadRunControlPublisher({
  channelId,
  rootEventId,
  conversationId,
  agentPubkey,
}: {
  channelId: string;
  rootEventId: string;
  conversationId: string;
  agentPubkey: string;
}) {
  const viewerPubkey = useIdentityQuery().data?.pubkey ?? "";
  const relayUrl = normalizeRelayUrl(
    useCommunities().activeCommunity?.relayUrl ?? "",
  );
  const owned = useCurrentOwnedAgentPubkeys(viewerPubkey).has(agentPubkey);
  const scopeKey = JSON.stringify([
    relayUrl,
    viewerPubkey,
    channelId,
    rootEventId,
    conversationId,
    agentPubkey,
    owned,
  ]);
  const epoch = React.useMemo(
    () => ({ key: scopeKey, current: true }),
    [scopeKey],
  );
  const [native, setNative] = React.useState<{
    epoch: typeof epoch;
    token: OwnerOperationScope | null;
    error?: string;
  } | null>(null);
  React.useLayoutEffect(() => {
    epoch.current = true;
    return () => {
      epoch.current = false;
    };
  }, [epoch]);
  React.useEffect(() => {
    if (!owned) return;
    let current = true;
    void captureOwnerOperationScope()
      .then((token) => {
        if (!current || !epoch.current) return;
        const expectedOrigin = new URL(relayUrl.replace(/^ws/, "http")).origin;
        if (
          token.scope.owner !== viewerPubkey ||
          token.scope.community !== expectedOrigin
        ) {
          setNative({
            epoch,
            token: null,
            error:
              "Owner or community changed. Reopen Activity to select a run.",
          });
        } else setNative({ epoch, token });
      })
      .catch(() => {
        if (current && epoch.current)
          setNative({
            epoch,
            token: null,
            error: "Run controls are unavailable. Reopen Activity to retry.",
          });
      });
    return () => {
      current = false;
    };
  }, [epoch, owned, relayUrl, viewerPubkey]);

  const token = native?.epoch === epoch ? native.token : null;
  const error = native?.epoch === epoch ? native.error : undefined;

  const buildSelection = (
    sessionId: string,
    turnId: string,
  ): ThreadRunSelection =>
    Object.freeze({
      relayUrl,
      viewerPubkey,
      channelId,
      rootEventId,
      conversationId,
      agentPubkey,
      sessionId,
      turnId,
    });

  /**
   * Publishers bound to exactly one committed run.
   *
   * Staleness is already fenced before a send can reach here: the caller
   * remounts `ThreadSelectedRunControls` under a `key={threadRunKey(run)}`
   * when the committed run changes (so a stale closure cannot even render
   * its send handler), `eligible()` re-reads `isLive(target)` at click time,
   * and the native call is scoped by `expectedScope`. What this guard alone
   * still owns is the epoch: `committed` and `run` are read from the same
   * render here, so they can never disagree — only `epoch.current` going
   * false (viewer/relay/scope change while the request was in flight) can
   * turn a send into `not_attempted`.
   */
  const publishersFor = (committed: ThreadRunCommitment) => {
    const guard = (): ThreadRunPublication | null => {
      if (!epoch.current || !committed) {
        return {
          status: "not_attempted",
          message: "The selected run changed before sending.",
        };
      }
      return null;
    };
    return {
      publishStop: async (
        run: ThreadRunSelection,
        requestId: string,
      ): Promise<ThreadRunPublication> => {
        const blocked = guard();
        if (blocked || !committed)
          return blocked ?? { status: "not_attempted" };
        return invokeTauri<ThreadRunPublication>(
          "send_scoped_observer_control",
          {
            agentPubkey: run.agentPubkey,
            expectedScope: committed.token,
            payload: {
              type: "cancel_turn",
              channelId: run.channelId,
              conversationId: run.conversationId,
              turnId: run.turnId,
              requestId,
            },
          },
        );
      },
      publishSteer: async (
        run: ThreadRunSelection,
        requestId: string,
        prompt: string,
      ): Promise<ThreadRunPublication> => {
        const blocked = guard();
        if (blocked || !committed)
          return blocked ?? { status: "not_attempted" };
        return invokeTauri<ThreadRunPublication>(
          "send_scoped_observer_control",
          {
            agentPubkey: run.agentPubkey,
            expectedScope: committed.token,
            payload: {
              type: "steer_turn",
              channelId: run.channelId,
              conversationId: run.conversationId,
              sessionId: run.sessionId,
              turnId: run.turnId,
              requestId,
              prompt,
            },
          },
        );
      },
    };
  };

  return {
    owned,
    relayUrl,
    viewerPubkey,
    epoch,
    token,
    error,
    buildSelection,
    publishersFor,
  };
}
