import * as React from "react";
import { toast } from "sonner";

import {
  useCanvasQuery,
  useChannelMembersQuery,
  useChannelsQuery,
} from "@/features/channels/hooks";
import { useSendMessageMutation } from "@/features/messages/hooks";
import { parseCrewFinding } from "@/features/messages/lib/parseCrewFinding";
import {
  resolveReviewerFromCanvas,
  reviewerStorageKey,
  reviewerThreadStorageKey,
} from "@/features/messages/lib/resolveReviewerFromCanvas";
import { useThreadForgeViewContext } from "@/features/messages/lib/threadForgeViewContextStore";
import type { ForgePullRequestDetail } from "@/shared/api/threadForgeTypes";
import { useIdentityQuery } from "@/shared/api/hooks";
import { truncatePubkey } from "@/shared/lib/pubkey";
import { Button } from "@/shared/ui/button";
import { Markdown } from "@/shared/ui/markdown";

export function ThreadPrHubBugs({
  channelId,
  pr,
  rootEventId,
}: {
  channelId: string | null;
  pr: ForgePullRequestDetail;
  rootEventId: string | null;
}) {
  const identityQuery = useIdentityQuery();
  const channelsQuery = useChannelsQuery();
  const channel =
    channelsQuery.data?.find((entry) => entry.id === channelId) ?? null;
  const sendMutation = useSendMessageMutation(channel, identityQuery.data);
  const canvasQuery = useCanvasQuery(channelId, Boolean(channelId));
  const membersQuery = useChannelMembersQuery(channelId, Boolean(channelId));
  const view = useThreadForgeViewContext();
  const [channelOverride, setChannelOverride] = React.useState<string | null>(
    () => (channelId ? readStored(reviewerStorageKey(channelId)) : null),
  );
  const [threadOverride, setThreadOverride] = React.useState<string | null>(
    () =>
      rootEventId ? readStored(reviewerThreadStorageKey(rootEventId)) : null,
  );
  const [picked, setPicked] = React.useState("");
  const [busy, setBusy] = React.useState(false);

  const resolution = resolveReviewerFromCanvas(canvasQuery.data, {
    channelPubkey: channelOverride,
    threadPubkey: threadOverride,
  });

  const findings = React.useMemo(() => {
    const messages = view?.messages ?? [];
    return messages.flatMap((message) => {
      const finding = parseCrewFinding(message.tags);
      if (!finding) return [];
      return [{ finding, message }];
    });
  }, [view?.messages]);

  const agents = (membersQuery.data ?? []).filter((member) => member.isAgent);

  async function runAnalysis() {
    if (resolution.status !== "held" || !channelId || !rootEventId) return;
    setBusy(true);
    try {
      const name =
        agents.find((agent) => agent.pubkey === resolution.pubkey)
          ?.displayName ?? truncatePubkey(resolution.pubkey);
      await sendMutation.mutateAsync({
        channelId,
        content: `@${name} please review ${pr.url} — ${pr.title}`,
        mentionPubkeys: [resolution.pubkey],
        parentEventId: rootEventId,
        threadHeadId: rootEventId,
      });
      toast.success("Asked the Reviewer to analyze this pull request.");
    } catch (error) {
      toast.error(
        error instanceof Error
          ? error.message
          : "Could not dispatch the Reviewer.",
      );
    } finally {
      setBusy(false);
    }
  }

  function persistPicker(scope: "channel" | "thread") {
    if (!picked) return;
    if (scope === "thread" && rootEventId) {
      writeStored(reviewerThreadStorageKey(rootEventId), picked);
      setThreadOverride(picked);
    }
    if (scope === "channel" && channelId) {
      writeStored(reviewerStorageKey(channelId), picked);
      setChannelOverride(picked);
    }
  }

  return (
    <div
      className="flex min-h-0 flex-1 flex-col overflow-hidden"
      data-testid="thread-pr-hub-bugs"
    >
      <div className="shrink-0 space-y-2 border-b border-border/60 p-3">
        {resolution.status === "held" ? (
          <>
            <p className="text-sm">
              Reviewer:{" "}
              <span
                className="font-medium"
                data-testid="thread-pr-hub-reviewer"
              >
                {truncatePubkey(resolution.pubkey)}
              </span>
              <span className="ml-2 text-2xs text-muted-foreground">
                via {resolution.source}
              </span>
            </p>
            <Button
              data-testid="thread-pr-hub-run-analysis"
              disabled={busy || !rootEventId}
              onClick={() => void runAnalysis()}
              size="sm"
              type="button"
            >
              Run analysis
            </Button>
          </>
        ) : (
          <div data-testid="thread-pr-hub-reviewer-picker">
            <p className="text-sm">
              No Reviewer is assigned on this channel canvas. Pick an agent.
            </p>
            <select
              className="mt-2 w-full rounded-md border border-border bg-background px-2 py-1 text-sm"
              onChange={(event) => setPicked(event.target.value)}
              value={picked}
            >
              <option value="">Select an agent</option>
              {agents.map((agent) => (
                <option key={agent.pubkey} value={agent.pubkey}>
                  {agent.displayName ?? truncatePubkey(agent.pubkey)}
                </option>
              ))}
            </select>
            <div className="mt-2 flex gap-2">
              <Button
                disabled={!picked}
                onClick={() => persistPicker("channel")}
                size="xs"
                type="button"
              >
                Save for channel
              </Button>
              <Button
                disabled={!picked || !rootEventId}
                onClick={() => persistPicker("thread")}
                size="xs"
                type="button"
                variant="outline"
              >
                This thread only
              </Button>
            </div>
          </div>
        )}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-3">
        {findings.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            No findings yet. Run analysis to ask the Reviewer; later messages
            tagged <code>crew-finding</code> appear here. Re-runs append.
          </p>
        ) : (
          findings.map(({ finding, message }) => (
            <article
              className="mb-3 rounded-lg border border-border/60 p-2"
              key={message.id}
            >
              <div className="flex items-center gap-2 text-2xs text-muted-foreground">
                <span className="rounded bg-muted px-1 font-medium uppercase text-foreground">
                  {finding.severity}
                </span>
                {finding.file ? (
                  <span className="font-mono">
                    {finding.file}
                    {finding.range ? `:${finding.range}` : ""}
                  </span>
                ) : null}
              </div>
              <div className="mt-1 text-sm">
                <Markdown content={message.body} />
              </div>
            </article>
          ))
        )}
      </div>
    </div>
  );
}

function readStored(key: string): string | null {
  try {
    return globalThis.localStorage?.getItem(key);
  } catch {
    return null;
  }
}

function writeStored(key: string, value: string): void {
  try {
    globalThis.localStorage?.setItem(key, value);
  } catch {
    // Persistence is best-effort.
  }
}
