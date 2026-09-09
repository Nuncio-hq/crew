import * as React from "react";
import { useIdentityQuery } from "@/shared/api/hooks";
import { useCommunities } from "@/features/communities/useCommunities";
import { useCurrentOwnedAgentPubkeys } from "@/features/home/useOwnedAgentPubkeys";
import { useActiveTurnSummariesForConversation } from "@/features/agents/activeConversationAgentTurnSummaries";
import {
  captureOwnerOperationScope,
  type OwnerOperationScope,
} from "@/shared/api/ownerOperations";
import { invokeTauri } from "@/shared/api/tauri";
import { normalizeRelayUrl } from "@/shared/lib/normalizeRelayUrl";
import {
  ThreadSelectedRunControls,
  type ThreadRunSelection,
} from "./ThreadSelectedRunControls";

type Publication = {
  status: "accepted" | "unknown" | "not_attempted";
  message?: string;
};

/** Explicit run selection over existing live observer identities and native owner capture. */
export function ThreadActivityRunControls({
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
  const [selected, setSelected] = React.useState<{
    epoch: typeof epoch;
    run: ThreadRunSelection;
    token: OwnerOperationScope;
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
  const summaries = useActiveTurnSummariesForConversation(conversationId);
  const runs =
    summaries.find((entry) => entry.agentPubkey === agentPubkey)?.runs ?? [];
  const chosen = selected?.epoch === epoch ? selected : null;
  const token = native?.epoch === epoch ? native.token : null;
  const keyFor = (run: { sessionId: string; turnId: string }) =>
    JSON.stringify([run.sessionId, run.turnId]);
  const candidates = [
    ...new Map(
      runs
        .filter((run) => run.sessionId && run.turnId)
        .map((run) => [keyFor(run), run]),
    ).values(),
  ];
  const value = chosen ? keyFor(chosen.run) : "";
  const stillListed = candidates.some((run) => keyFor(run) === value);
  const publishStop = async (
    run: ThreadRunSelection,
    requestId: string,
  ): Promise<Publication> => {
    if (!epoch.current || !chosen || keyFor(chosen.run) !== keyFor(run)) {
      return {
        status: "not_attempted",
        message: "The selected run changed before sending.",
      };
    }
    return invokeTauri<Publication>("send_scoped_observer_control", {
      agentPubkey: run.agentPubkey,
      expectedScope: chosen.token,
      payload: {
        type: "cancel_turn",
        channelId: run.channelId,
        conversationId: run.conversationId,
        turnId: run.turnId,
        requestId,
      },
    });
  };
  if (!owned) return null;
  return (
    <div className="border-b border-border/60 p-2">
      <label className="flex items-center gap-2 text-sm">
        Live run
        <select
          aria-label="Activity live run"
          value={value}
          disabled={!token}
          onChange={(event) => {
            const run = candidates.find(
              (candidate) => keyFor(candidate) === event.target.value,
            );
            if (!run || !token) {
              setSelected(null);
              return;
            }
            setSelected({
              epoch,
              token,
              run: Object.freeze({
                relayUrl,
                viewerPubkey,
                channelId,
                rootEventId,
                conversationId,
                agentPubkey,
                sessionId: run.sessionId,
                turnId: run.turnId,
              }),
            });
          }}
        >
          <option value="">Select a live run</option>
          {chosen && !stillListed ? (
            <option value={value}>Selected run finished or unavailable</option>
          ) : null}
          {candidates.map((run) => (
            <option key={keyFor(run)} value={keyFor(run)}>
              Run {run.turnId.slice(0, 8)}
            </option>
          ))}
        </select>
      </label>
      {native?.epoch === epoch && native.error ? (
        <p role="status">{native.error}</p>
      ) : null}
      <ThreadSelectedRunControls
        selection={chosen?.run ?? null}
        publishStop={publishStop}
      />
      <p className="text-xs text-muted-foreground">
        Steer is unavailable until this runtime supports targeting an exact run.
      </p>
    </div>
  );
}
