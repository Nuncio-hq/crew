import * as React from "react";
import { toast } from "sonner";

import { createForgePr } from "@/shared/api/threadForge";
import { reloadProjectThreadGitHubStore } from "@/features/messages/lib/projectThreadGitHubStore";
import { invalidateThreadForgePullRequestStore } from "@/features/messages/lib/threadForgePullRequestStore";
import type { ThreadForgeHubSubject } from "@/features/messages/lib/threadForgeHubSubjectStore";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import { Textarea } from "@/shared/ui/textarea";

export function ThreadPrHubNoPullRequest({
  subject,
}: {
  subject: Extract<ThreadForgeHubSubject, { kind: "empty" }>;
}) {
  const [title, setTitle] = React.useState(subject.branch);
  const [body, setBody] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const canCreate = Boolean(
    subject.owner && subject.name && subject.worktreePath,
  );

  async function onCreate() {
    if (!subject.owner || !subject.name || !subject.worktreePath) return;
    setBusy(true);
    try {
      await createForgePr({
        owner: subject.owner,
        name: subject.name,
        worktreePath: subject.worktreePath,
        title,
        body,
        base: subject.baseRef ?? "main",
        head: subject.branch,
      });
      invalidateThreadForgePullRequestStore();
      reloadProjectThreadGitHubStore();
      toast.success("Created the pull request.");
    } catch (error) {
      toast.error(
        error instanceof Error
          ? error.message
          : "Could not create the pull request.",
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <div
      className="flex min-h-0 flex-1 flex-col gap-3 p-4"
      data-testid="thread-pr-hub-no-pr"
    >
      <h2 className="text-sm font-semibold">No pull request yet</h2>
      <p className="text-sm text-muted-foreground">
        Branch <span className="font-mono text-2xs">{subject.branch}</span>
        {subject.worktreePath ? (
          <>
            {" "}
            is checked out at{" "}
            <span className="font-mono text-2xs">{subject.worktreePath}</span>
          </>
        ) : (
          ". The worktree is not on disk."
        )}
      </p>
      {canCreate ? (
        <>
          <Input
            onChange={(event) => setTitle(event.target.value)}
            placeholder="Title"
            value={title}
          />
          <Textarea
            onChange={(event) => setBody(event.target.value)}
            placeholder="Description"
            value={body}
          />
          <Button
            data-testid="thread-pr-hub-create-pr"
            disabled={busy || !title.trim()}
            onClick={() => void onCreate()}
            type="button"
          >
            Create PR
          </Button>
        </>
      ) : (
        <p className="text-sm text-muted-foreground">
          Bind a repository path to create a pull request from this branch.
        </p>
      )}
    </div>
  );
}
