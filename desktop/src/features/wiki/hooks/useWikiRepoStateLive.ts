import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";
import { getRelaySelf } from "@/features/moderation/lib/relaySelf";
import type { OwnerOperationScope } from "@/shared/api/ownerOperations";
import { assertWikiSnapshotScope } from "@/shared/api/wikiSnapshot";
import { relayClient } from "@/shared/api/relayClient";
import { KIND_REPO_STATE } from "@/shared/constants/kinds";

/** Push events request a scoped native reread; their payload is never authority. */
export function useWikiRepoStateLive(
  scope: OwnerOperationScope | null,
  repositorySignature: string,
  refetch: (options: {
    cancelRefetch: boolean;
    throwOnError: boolean;
  }) => Promise<unknown>,
) {
  const queryClient = useQueryClient();
  const [error, setError] = React.useState<string | null>(null);
  const [attempt, retry] = React.useReducer((value: number) => value + 1, 0);
  // biome-ignore lint/correctness/useExhaustiveDependencies: Retry intentionally replaces the failed subscription.
  React.useEffect(() => {
    setError(null);
    if (!scope) return;
    const repositories: Array<{ owner: string; dtag: string }> =
      JSON.parse(repositorySignature);
    if (repositories.length === 0) return;
    let disposed = false;
    let unsubscribe: (() => Promise<void>) | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let reading = false;
    let dirty = false;
    let revision = 0;
    let connected = false;
    const fail = () => {
      if (!disposed)
        setError("Automatic Wiki updates are interrupted. Retry live updates.");
    };
    const schedule = () => {
      if (disposed) return;
      dirty = true;
      revision += 1;
      if (reading || timer !== undefined) return;
      timer = setTimeout(() => {
        timer = undefined;
        void refresh();
      }, 250);
    };
    const refresh = async () => {
      if (disposed || reading) return;
      reading = true;
      dirty = false;
      const startedAt = revision;
      try {
        await assertWikiSnapshotScope(scope);
        if (disposed) return;
        const joinedOlderRead =
          queryClient.getQueryState([
            "crew-wiki-events",
            "repos",
            scope.scope.owner,
            scope.scope.community,
            scope.workspace_generation,
            scope.identity_generation,
          ])?.fetchStatus === "fetching";
        // Cancel a pre-event read where QueryClient can; an initial read with
        // no data may instead be joined and then needs one trailing read.
        await refetch({ cancelRefetch: true, throwOnError: true });
        await assertWikiSnapshotScope(scope);
        if (joinedOlderRead) dirty = true;
        if (!disposed && connected) setError(null);
      } catch {
        fail();
      } finally {
        reading = false;
        // An event during the read demands one trailing read. Failures alone
        // never spin: only a new event, reconnect, or explicit Retry wakes us.
        if (!disposed && (dirty || startedAt !== revision)) schedule();
      }
    };
    const close = (dispose: () => Promise<void>) => {
      // RelayClient retires its local alias before awaiting the wire CLOSE.
      void dispose().catch(fail);
    };
    void (async () => {
      try {
        const relaySelf = await getRelaySelf();
        await assertWikiSnapshotScope(scope);
        if (disposed) return;
        const authors = [
          ...new Set([
            ...repositories.map(({ owner }) => owner.toLowerCase()),
            ...(relaySelf ? [relaySelf.toLowerCase()] : []),
          ]),
        ];
        const dispose = await relayClient.subscribeLive(
          {
            kinds: [KIND_REPO_STATE],
            authors,
            "#d": [...new Set(repositories.map(({ dtag }) => dtag))],
            limit: 0,
          },
          (event) => {
            if (disposed || event.kind !== KIND_REPO_STATE) return;
            const author = event.pubkey.toLowerCase();
            if (!authors.includes(author)) return;
            const matches = repositories.some(
              ({ owner, dtag }) =>
                event.tags.some((tag) => tag[0] === "d" && tag[1] === dtag) &&
                (author === owner.toLowerCase() ||
                  (author === relaySelf?.toLowerCase() &&
                    event.tags.some(
                      (tag) =>
                        tag[0] === "a" && tag[1] === `30617:${owner}:${dtag}`,
                    ))),
            );
            if (matches) schedule();
          },
          (status) => {
            connected = status.state === "open";
            // An actual open closes the history/live gap. subscribeLive can
            // resolve after a readiness timeout while still recovering.
            if (status.state === "open") schedule();
            else fail();
          },
        );
        if (disposed) close(dispose);
        else {
          unsubscribe = dispose;
        }
      } catch {
        fail();
      }
    })();
    return () => {
      disposed = true;
      clearTimeout(timer);
      if (unsubscribe) close(unsubscribe);
    };
  }, [scope, repositorySignature, refetch, attempt, queryClient]);
  return { error, retry };
}
