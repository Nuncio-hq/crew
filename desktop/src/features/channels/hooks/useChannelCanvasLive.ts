import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";
import { relayClient } from "@/shared/api/relayClient";
import {
  captureOwnerOperationScope,
  sameOwnerOperationScope,
  type OwnerOperationScope,
} from "@/shared/api/ownerOperations";

/** Canvas events invalidate the existing query; event payloads never become authority. */
export function useChannelCanvasLive(
  channelId: string | null,
  scopeKey: string,
) {
  const queryClient = useQueryClient();
  const [error, setError] = React.useState<string | null>(null);
  React.useEffect(() => {
    if (!channelId || !scopeKey) return;
    let disposed = false;
    let stopped = false;
    let unsubscribe: (() => Promise<void>) | undefined;
    let captured: OwnerOperationScope | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let reading = false;
    let dirty = false;
    setError(null);

    const retire = async () => {
      stopped = true;
      dirty = false;
      if (timer !== undefined) {
        clearTimeout(timer);
        timer = undefined;
      }
      const close = unsubscribe;
      unsubscribe = undefined;
      // RelayClient deletes local aliases before awaiting its wire CLOSE.
      if (close) await close().catch(() => {});
    };
    const current = async () => {
      let token: OwnerOperationScope;
      try {
        token = await captureOwnerOperationScope();
      } catch (cause) {
        await retire();
        throw cause;
      }
      if (
        !disposed &&
        !stopped &&
        captured !== undefined &&
        sameOwnerOperationScope(token, captured)
      )
        return true;
      await retire();
      if (!disposed)
        setError(
          "Community or identity changed. Reopen the channel to resume live updates.",
        );
      return false;
    };
    const schedule = () => {
      if (disposed || stopped) return;
      dirty = true;
      if (reading || timer !== undefined) return;
      timer = setTimeout(() => {
        timer = undefined;
        void refresh();
      }, 250);
    };
    const refresh = async () => {
      if (disposed || stopped || reading) return;
      reading = true;
      dirty = false;
      try {
        if (!(await current())) return;
        const joinedOlderRead =
          queryClient.getQueryState(["channel-canvas", channelId])
            ?.fetchStatus === "fetching";
        await queryClient.invalidateQueries(
          { queryKey: ["channel-canvas", channelId] },
          { cancelRefetch: false, throwOnError: true },
        );
        if (await current()) {
          // cancelRefetch:false may join a request started before this event.
          // After a successful join, demand one bounded post-event read.
          if (joinedOlderRead) dirty = true;
          setError(null);
        }
      } catch {
        if (!disposed)
          setError(
            "Live canvas refresh failed. Use Refresh canvas to check the current version.",
          );
      } finally {
        reading = false;
        if (dirty) schedule();
      }
    };

    void (async () => {
      try {
        captured = await captureOwnerOperationScope();
        if (disposed) return;
        const dispose = await relayClient.subscribeLive(
          { kinds: [40100], "#h": [channelId], limit: 1 },
          (event) => {
            if (
              event.kind === 40100 &&
              event.tags.some((tag) => tag[0] === "h" && tag[1] === channelId)
            )
              schedule();
          },
          (status) => {
            if (disposed || stopped) return;
            if (status.state === "open") schedule();
            else
              setError(
                "Live canvas updates are interrupted. Use Refresh canvas to check the current version.",
              );
          },
        );
        unsubscribe = dispose;
        if (!(await current())) return;
        // Subscribe before refreshing so history and live delivery overlap.
        schedule();
      } catch {
        await retire();
        if (!disposed)
          setError(
            "Live canvas updates are unavailable. Use Refresh canvas to check the current version.",
          );
      }
    })();
    return () => {
      disposed = true;
      void retire();
    };
  }, [channelId, scopeKey, queryClient]);
  return error;
}
