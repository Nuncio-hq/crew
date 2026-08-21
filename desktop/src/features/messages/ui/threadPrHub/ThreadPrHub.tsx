import { Check, LoaderCircle, RefreshCw, X } from "lucide-react";
import * as React from "react";

import { parseCrewFinding } from "@/features/messages/lib/parseCrewFinding";
import type { ThreadForgeHubSubject } from "@/features/messages/lib/threadForgeHubSubjectStore";
import { useThreadForgePullRequest } from "@/features/messages/lib/threadForgePullRequestStore";
import { useThreadForgeViewContext } from "@/features/messages/lib/threadForgeViewContextStore";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/shared/ui/tabs";
import { cn } from "@/shared/lib/cn";

import { FORGE_TAB_TRIGGER_CLASS, formatIsoRelative } from "./forgeHubCopy";
import { summarizeChecksTab } from "./forgeCheckGroups";
import { ThreadPrHubBugs } from "./ThreadPrHubBugs";
import { ThreadPrHubChanges } from "./ThreadPrHubChanges";
import { ThreadPrHubChecks } from "./ThreadPrHubChecks";
import { ThreadPrHubCommits } from "./ThreadPrHubCommits";
import { ThreadPrHubDegraded } from "./ThreadPrHubDegraded";
import { ThreadPrHubDescription } from "./ThreadPrHubDescription";
import { ThreadPrHubDiscussion } from "./ThreadPrHubDiscussion";
import { ThreadPrHubHeader } from "./ThreadPrHubHeader";
import { ThreadPrHubNoPullRequest } from "./ThreadPrHubNoPullRequest";

export function ThreadPrHub({ subject }: { subject: ThreadForgeHubSubject }) {
  const ref =
    subject.kind === "pr"
      ? {
          owner: subject.owner,
          name: subject.name,
          number: subject.number,
        }
      : null;
  const worktreePath = subject.worktreePath;
  const baseRef =
    subject.kind === "pr" ? undefined : (subject.baseRef ?? undefined);
  const { refresh, snapshot } = useThreadForgePullRequest(
    ref,
    worktreePath,
    baseRef,
  );
  const view = useThreadForgeViewContext();
  const [tab, setTab] = React.useState("changes");
  const [refreshing, setRefreshing] = React.useState(false);
  const lastRefresh = React.useRef(Date.now());
  const findingCount = (view?.messages ?? []).filter((message) =>
    parseCrewFinding(message.tags),
  ).length;

  const onRefresh = React.useCallback(async () => {
    setRefreshing(true);
    lastRefresh.current = Date.now();
    await refresh();
    setRefreshing(false);
  }, [refresh]);

  if (subject.kind === "empty") {
    return <ThreadPrHubNoPullRequest subject={subject} />;
  }

  if (snapshot.status === "pending") {
    return (
      <div className="flex min-w-0 flex-1 items-center justify-center text-sm text-muted-foreground">
        <LoaderCircle className="mr-2 h-4 w-4 animate-spin" />
        Loading pull request…
      </div>
    );
  }

  const { detail, diff } = snapshot;
  if (
    detail.availability === "cli-missing" ||
    detail.availability === "cli-failed" ||
    detail.availability === "rate-limited"
  ) {
    return (
      <ThreadPrHubDegraded
        availability={detail.availability}
        message={detail.message}
        rateLimitedUntil={detail.rateLimitedUntil}
        onRecheck={() => void onRefresh()}
        refreshDisabled={detail.availability === "rate-limited"}
      />
    );
  }

  const pr = detail.detail;
  if (!pr) {
    return (
      <ThreadPrHubDegraded
        availability="cli-failed"
        message={detail.message ?? "Pull request was not found."}
        rateLimitedUntil={null}
        onRecheck={() => void onRefresh()}
        refreshDisabled={false}
      />
    );
  }

  const checksSummary = summarizeChecksTab(pr.checks);
  const reviewCount =
    pr.comments.length +
    pr.reviews.length +
    pr.reviewThreads.reduce((sum, thread) => sum + thread.comments.length, 0);

  return (
    <div className="relative flex min-h-0 min-w-0 flex-1 flex-col">
      <ThreadPrHubHeader
        diffSource={diff?.diff?.source ?? null}
        name={subject.name}
        onRefresh={() => void onRefresh()}
        owner={subject.owner}
        pr={pr}
        refreshDisabled={false}
        refreshing={refreshing}
        refreshedLabel={`refreshed ${formatIsoRelative(new Date(lastRefresh.current).toISOString())}`}
      />
      <Tabs
        className="flex min-h-0 min-w-0 flex-1 flex-col"
        onValueChange={setTab}
        value={tab}
      >
        <TabsList className="h-9 w-full justify-start gap-0.5 overflow-x-auto border-b border-border/50 bg-transparent px-2 scrollbar-none">
          <HubTab label="Changes" value="changes" count={pr.files.length} />
          <HubTab label="Description" value="description" count={null} />
          <HubTab label="Commits" value="commits" count={pr.commits.length} />
          <ChecksHubTab summary={checksSummary} />
          <HubTab label="Reviews" value="discussion" count={reviewCount} />
          {findingCount > 0 ? (
            <HubTab label="Bugs" value="bugs" count={findingCount} />
          ) : null}
        </TabsList>
        <TabsContent
          className="mt-0 min-h-0 flex-1 overflow-hidden"
          value="changes"
        >
          <ThreadPrHubChanges
            diff={diff}
            files={pr.files}
            onRefresh={() => void onRefresh()}
            pr={pr}
            refIdentity={subject}
          />
        </TabsContent>
        <TabsContent
          className="mt-0 min-h-0 flex-1 overflow-hidden"
          value="bugs"
        >
          <ThreadPrHubBugs
            channelId={subject.channelId}
            pr={pr}
            rootEventId={subject.rootEventId}
          />
        </TabsContent>
        <TabsContent
          className="mt-0 min-h-0 flex-1 overflow-auto p-3"
          value="description"
        >
          <ThreadPrHubDescription body={pr.body} />
        </TabsContent>
        <TabsContent
          className="mt-0 min-h-0 flex-1 overflow-hidden"
          value="discussion"
        >
          <ThreadPrHubDiscussion
            onRefresh={() => void onRefresh()}
            pr={pr}
            refIdentity={subject}
          />
        </TabsContent>
        <TabsContent
          className="mt-0 min-h-0 flex-1 overflow-hidden"
          value="commits"
        >
          <ThreadPrHubCommits
            commits={pr.commits}
            diff={diff}
            worktreePath={subject.worktreePath}
          />
        </TabsContent>
        <TabsContent
          className="mt-0 min-h-0 flex-1 overflow-hidden"
          value="checks"
        >
          <ThreadPrHubChecks
            checks={pr.checks}
            onRefresh={() => void onRefresh()}
            refIdentity={subject}
          />
        </TabsContent>
      </Tabs>
      {refreshing ? (
        <div className="pointer-events-none absolute right-4 top-4">
          <RefreshCw className="h-4 w-4 animate-spin text-muted-foreground" />
        </div>
      ) : null}
    </div>
  );
}

function HubTab({
  count,
  label,
  value,
}: {
  count: number | null;
  label: string;
  value: string;
}) {
  return (
    <TabsTrigger className={cn(FORGE_TAB_TRIGGER_CLASS)} value={value}>
      {label}
      {count === null ? null : (
        <span className="ml-1 tabular-nums text-muted-foreground">{count}</span>
      )}
    </TabsTrigger>
  );
}

/** Cursor: "Checks 8/14 Running" with circle status glyph. */
function ChecksHubTab({
  summary,
}: {
  summary: ReturnType<typeof summarizeChecksTab>;
}) {
  return (
    <TabsTrigger
      className={cn(FORGE_TAB_TRIGGER_CLASS)}
      data-testid="thread-pr-hub-tab-checks"
      value="checks"
    >
      <span className="inline-flex items-center gap-1.5">
        <ChecksTabGlyph kind={summary.kind} />
        <span>Checks</span>
        <span
          className={cn(
            "tabular-nums",
            summary.kind === "failed"
              ? "text-destructive"
              : summary.kind === "running"
                ? "text-attention"
                : summary.kind === "passed"
                  ? "text-success"
                  : "text-muted-foreground",
          )}
        >
          {summary.label}
        </span>
      </span>
    </TabsTrigger>
  );
}

function ChecksTabGlyph({
  kind,
}: {
  kind: ReturnType<typeof summarizeChecksTab>["kind"];
}) {
  switch (kind) {
    case "running":
      return (
        <LoaderCircle className="h-3.5 w-3.5 animate-spin text-attention" />
      );
    case "failed":
      return <X className="h-3.5 w-3.5 text-destructive" strokeWidth={2.5} />;
    case "passed":
      return <Check className="h-3.5 w-3.5 text-success" strokeWidth={2.5} />;
    case "empty":
      return null;
    default: {
      const _exhaustive: never = kind;
      return _exhaustive;
    }
  }
}
