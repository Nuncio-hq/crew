import * as React from "react";
import { toast } from "sonner";

import { commentForgePr, reviewForgePr } from "@/shared/api/threadForge";
import { invalidateThreadForgePullRequestStore } from "@/features/messages/lib/threadForgePullRequestStore";
import type { ThreadForgeHubSubject } from "@/features/messages/lib/threadForgeHubSubjectStore";
import type {
  ForgePullRequestDetail,
  ForgeReviewEvent,
} from "@/shared/api/threadForgeTypes";
import { Button } from "@/shared/ui/button";
import { Textarea } from "@/shared/ui/textarea";
import { Markdown } from "@/shared/ui/markdown";

import { formatIsoRelative } from "./forgeHubCopy";

export function ThreadPrHubDiscussion({
  onRefresh,
  pr,
  refIdentity,
}: {
  onRefresh: () => void;
  pr: ForgePullRequestDetail;
  refIdentity: Extract<ThreadForgeHubSubject, { kind: "pr" }>;
}) {
  const [body, setBody] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const items = React.useMemo(() => collectDiscussion(pr), [pr]);

  async function submit(event: ForgeReviewEvent | "issue-comment") {
    if (event !== "approve" && !body.trim()) return;
    setBusy(true);
    try {
      if (event === "issue-comment") {
        await commentForgePr({
          owner: refIdentity.owner,
          name: refIdentity.name,
          number: refIdentity.number,
          body,
        });
      } else {
        await reviewForgePr({
          owner: refIdentity.owner,
          name: refIdentity.name,
          number: refIdentity.number,
          event,
          body,
        });
      }
      setBody("");
      invalidateThreadForgePullRequestStore();
      onRefresh();
    } catch (error) {
      toast.error(
        error instanceof Error ? error.message : "Could not post to GitHub.",
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <div
      className="flex min-h-0 flex-1 flex-col"
      data-testid="thread-pr-hub-discussion"
    >
      <div className="min-h-0 flex-1 overflow-y-auto p-3">
        {items.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            No GitHub discussion yet.
          </p>
        ) : (
          items.map((item) => (
            <article
              className="mb-3 rounded-lg border border-border/60 p-2"
              key={item.id}
            >
              <div className="flex items-center gap-2 text-2xs text-muted-foreground">
                <span className="font-medium text-foreground">
                  {item.author ?? "unknown"}
                </span>
                <span>{item.kind}</span>
                {item.anchor ? (
                  <span className="rounded bg-muted px-1 font-mono">
                    {item.anchor}
                  </span>
                ) : null}
                <span>{formatIsoRelative(item.at)}</span>
              </div>
              {item.body ? (
                <div className="mt-1 text-sm">
                  <Markdown content={item.body} />
                </div>
              ) : null}
            </article>
          ))
        )}
      </div>
      <div className="shrink-0 border-t border-border/60 p-3">
        <p className="mb-1 text-2xs text-muted-foreground">
          This box posts to GitHub. The thread composer on the left posts to the
          room.
        </p>
        <Textarea
          data-testid="thread-pr-hub-github-comment"
          onChange={(event) => setBody(event.target.value)}
          placeholder="Comment on GitHub"
          value={body}
        />
        <div className="mt-2 flex flex-wrap gap-2">
          <Button
            data-testid="thread-pr-hub-github-comment-submit"
            disabled={busy || !body.trim()}
            onClick={() => void submit("issue-comment")}
            size="sm"
            type="button"
          >
            Comment on GitHub
          </Button>
          <Button
            data-testid="thread-pr-hub-approve"
            disabled={busy}
            onClick={() => void submit("approve")}
            size="sm"
            type="button"
            variant="outline"
          >
            Approve
          </Button>
          <Button
            data-testid="thread-pr-hub-review-comment"
            disabled={busy || !body.trim()}
            onClick={() => void submit("comment")}
            size="sm"
            type="button"
            variant="outline"
          >
            Review comment
          </Button>
          <Button
            disabled={busy || !body.trim()}
            onClick={() => void submit("request-changes")}
            size="sm"
            type="button"
            variant="outline"
          >
            Request changes
          </Button>
        </div>
      </div>
    </div>
  );
}

function collectDiscussion(pr: ForgePullRequestDetail) {
  const items: Array<{
    id: string;
    author: string | null;
    body: string;
    kind: string;
    anchor: string | null;
    at: string;
  }> = [];
  for (const comment of pr.comments) {
    items.push({
      id: comment.id,
      author: comment.author?.login ?? null,
      body: comment.body,
      kind: "comment",
      anchor: null,
      at: comment.createdAt,
    });
  }
  for (const review of pr.reviews) {
    items.push({
      id: review.id,
      author: review.author?.login ?? null,
      body: review.body,
      kind: review.state.toLowerCase(),
      anchor: null,
      at: review.submittedAt ?? "",
    });
  }
  for (const thread of pr.reviewThreads) {
    const anchor =
      thread.path && thread.line
        ? `${thread.path}:${thread.line}`
        : thread.path;
    for (const comment of thread.comments) {
      items.push({
        id: comment.id,
        author: comment.author?.login ?? null,
        body: comment.body,
        kind: thread.isResolved ? "resolved" : "review thread",
        anchor,
        at: comment.createdAt,
      });
    }
  }
  items.sort((a, b) => a.at.localeCompare(b.at));
  return items;
}
