import * as React from "react";

import { useCommunities } from "@/features/communities/useCommunities";
import { useIdentityQuery } from "@/shared/api/hooks";
import { normalizePubkey } from "@/shared/lib/pubkey";

/** Native-supported scope assertions for one directory/profile action. */
export type AgentControlNativeScope = {
  expectedRelayUrl: string;
  expectedSignerPubkey: string;
};

/**
 * Captures the rendered community, signer and profile target together. Local
 * lifetime tokens fence completion; native assertions fence the actual write.
 * No operation state survives this component or becomes a second registry.
 */
export function useAgentControlScope(targetKey: string | null = null) {
  const { activeCommunity } = useCommunities();
  const identity = useIdentityQuery();
  const expectedRelayUrl = activeCommunity?.relayUrl?.trim() || "";
  const expectedSignerPubkey = normalizePubkey(identity.data?.pubkey ?? "");
  const latest = React.useRef({
    expectedRelayUrl,
    expectedSignerPubkey,
    targetKey,
  });
  if (
    latest.current.expectedRelayUrl !== expectedRelayUrl ||
    latest.current.expectedSignerPubkey !== expectedSignerPubkey ||
    latest.current.targetKey !== targetKey
  ) {
    latest.current = { expectedRelayUrl, expectedSignerPubkey, targetKey };
  }
  const rendered = latest.current;
  const mounted = React.useRef(false);
  const lifetime = React.useRef(0);
  React.useEffect(() => {
    mounted.current = true;
    lifetime.current += 1;
    return () => {
      mounted.current = false;
    };
  }, []);

  return React.useCallback(
    (isTargetCurrent: () => boolean = () => true) => {
      const capturedLifetime = lifetime.current;
      const isCurrent = () =>
        mounted.current &&
        lifetime.current === capturedLifetime &&
        latest.current === rendered &&
        isTargetCurrent();
      return {
        isCurrent,
        nativeScope(): AgentControlNativeScope {
          if (!isCurrent())
            throw new Error(
              "The selected agent or community changed. Try the action again.",
            );
          if (!rendered.expectedRelayUrl || !rendered.expectedSignerPubkey) {
            throw new Error(
              "The community or identity is still loading. Try the action again when connected.",
            );
          }
          return {
            expectedRelayUrl: rendered.expectedRelayUrl,
            expectedSignerPubkey: rendered.expectedSignerPubkey,
          };
        },
      };
    },
    [rendered],
  );
}
