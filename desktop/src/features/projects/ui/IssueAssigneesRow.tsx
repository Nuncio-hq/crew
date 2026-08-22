import { Search, X } from "lucide-react";
import * as React from "react";
import { toast } from "sonner";

import { useIsManagedAgent } from "@/features/agent-memory/hooks";
import type { Repository } from "@/features/projects/hooks";
import {
  useAssignProjectIssueMutation,
  useUnassignProjectIssueMutation,
} from "@/features/projects/issueAssignments";
import type { ProjectIssue } from "@/features/projects/projectIssues.mjs";
import { useUserSearchQuery } from "@/features/profile/hooks";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { useIdentityQuery } from "@/shared/api/hooks";
import { normalizePubkey, truncatePubkey } from "@/shared/lib/pubkey";
import { Button } from "@/shared/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/shared/ui/dialog";
import { Input } from "@/shared/ui/input";
import { UserAvatar } from "@/shared/ui/UserAvatar";

function labelForPubkey(pubkey: string, profiles?: UserProfileLookup) {
  const profile = profiles?.[normalizePubkey(pubkey)];
  return (
    profile?.displayName?.trim() ||
    profile?.nip05Handle?.trim() ||
    truncatePubkey(pubkey)
  );
}

export function IssueAssigneeFacepile({
  assignees,
  profiles,
}: {
  assignees: string[];
  profiles?: UserProfileLookup;
}) {
  if (assignees.length === 0) return null;
  return (
    <span className="flex shrink-0 -space-x-1">
      {assignees.slice(0, 3).map((pubkey) => (
        <UserAvatar
          avatarUrl={profiles?.[normalizePubkey(pubkey)]?.avatarUrl ?? null}
          className="rounded-full ring-1 ring-border"
          displayName={labelForPubkey(pubkey, profiles)}
          fallbackDelayMs={0}
          key={pubkey}
          size="xs"
        />
      ))}
    </span>
  );
}

export function IssueAssigneesRow({
  issue,
  profiles,
  project,
}: {
  issue: ProjectIssue;
  profiles?: UserProfileLookup;
  project: Repository;
}) {
  const identityQuery = useIdentityQuery();
  const viewer = identityQuery.data?.pubkey
    ? normalizePubkey(identityQuery.data.pubkey)
    : null;
  const canAssignOthers =
    viewer === normalizePubkey(issue.author) ||
    viewer === normalizePubkey(project.owner);
  const isManagedAgentOwner = useIsManagedAgent(project.owner) === true;
  const canManageAssignees =
    viewer !== null && (canAssignOthers || isManagedAgentOwner);
  const [pickerOpen, setPickerOpen] = React.useState(false);
  const [query, setQuery] = React.useState("");
  const assignMutation = useAssignProjectIssueMutation(project);
  const unassignMutation = useUnassignProjectIssueMutation(project);
  const current = React.useMemo(
    () => new Set(issue.assignees.map(normalizePubkey)),
    [issue.assignees],
  );
  const search = useUserSearchQuery(React.useDeferredValue(query.trim()), {
    allowEmpty: true,
    enabled: canManageAssignees && pickerOpen,
    limit: 50,
  });
  const candidates = (search.data ?? []).filter(
    (candidate) => !current.has(normalizePubkey(candidate.pubkey)),
  );
  const pending = assignMutation.isPending || unassignMutation.isPending;

  async function assign(pubkey: string, assigneeLabel: string) {
    if (!viewer) return;
    try {
      await assignMutation.mutateAsync({
        assignees: [pubkey],
        assigneeLabel,
        issue,
        signerPubkey: viewer,
        signAsManagedOwner:
          isManagedAgentOwner && viewer !== normalizePubkey(project.owner),
      });
      setPickerOpen(false);
      toast.success("Issue assigned.");
    } catch (error) {
      toast.error(
        error instanceof Error ? error.message : "Failed to assign issue.",
      );
    }
  }

  async function unassign(pubkey: string) {
    if (!viewer) return;
    try {
      await unassignMutation.mutateAsync({
        assignees: [pubkey],
        assigneeLabel: labelForPubkey(pubkey, profiles),
        issue,
        signerPubkey: viewer,
        signAsManagedOwner:
          isManagedAgentOwner && viewer !== normalizePubkey(project.owner),
      });
      toast.success("Issue unassigned.");
    } catch (error) {
      toast.error(
        error instanceof Error ? error.message : "Failed to unassign issue.",
      );
    }
  }

  const canSelfAssign = viewer !== null && !current.has(viewer);
  if (issue.assignees.length === 0 && !canAssignOthers && !canSelfAssign) {
    return null;
  }

  return (
    <div className="flex min-w-0 flex-wrap items-center gap-1.5">
      {issue.assignees.map((pubkey) => {
        const normalized = normalizePubkey(pubkey);
        const canUnassign = canManageAssignees || viewer === normalized;
        const avatar = (
          <UserAvatar
            avatarUrl={profiles?.[normalized]?.avatarUrl ?? null}
            displayName={labelForPubkey(pubkey, profiles)}
            key={pubkey}
            size="xs"
          />
        );
        return canUnassign ? (
          <button
            aria-label={`Unassign ${labelForPubkey(pubkey, profiles)}`}
            className="group relative inline-flex rounded-full"
            disabled={pending}
            key={pubkey}
            onClick={() => void unassign(pubkey)}
            type="button"
          >
            {avatar}
            <span className="absolute inset-0 flex items-center justify-center rounded-full bg-background/80 opacity-0 group-hover:opacity-100">
              <X className="h-3 w-3" />
            </span>
          </button>
        ) : (
          <span key={pubkey}>{avatar}</span>
        );
      })}
      {canSelfAssign && viewer ? (
        <Button
          disabled={pending}
          onClick={() => void assign(viewer, labelForPubkey(viewer, profiles))}
          size="xs"
          type="button"
          variant="ghost"
        >
          Assign to me
        </Button>
      ) : null}
      {canManageAssignees ? (
        <Dialog onOpenChange={setPickerOpen} open={pickerOpen}>
          <DialogTrigger asChild>
            <Button disabled={pending} size="xs" type="button" variant="ghost">
              Assign
            </Button>
          </DialogTrigger>
          <DialogContent className="max-w-md gap-0 overflow-hidden p-0">
            <DialogHeader className="border-b px-6 py-5">
              <DialogTitle>Assign issue</DialogTitle>
              <DialogDescription>
                Choose a person or agent to work on this issue.
              </DialogDescription>
            </DialogHeader>
            <div className="flex items-center gap-2 border-b px-6 py-3">
              <Search className="h-3.5 w-3.5 text-muted-foreground" />
              <Input
                autoFocus
                className="border-0 shadow-none focus-visible:ring-0"
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Search people and agents"
                value={query}
              />
            </div>
            <div className="max-h-72 overflow-y-auto p-2">
              {candidates.map((candidate) => (
                <button
                  className="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left hover:bg-accent"
                  disabled={pending}
                  key={candidate.pubkey}
                  onClick={() =>
                    void assign(
                      candidate.pubkey,
                      candidate.displayName ??
                        candidate.nip05Handle ??
                        truncatePubkey(candidate.pubkey),
                    )
                  }
                  type="button"
                >
                  <UserAvatar
                    avatarUrl={candidate.avatarUrl}
                    displayName={
                      candidate.displayName ?? truncatePubkey(candidate.pubkey)
                    }
                    size="xs"
                  />
                  <span className="truncate text-sm">
                    {candidate.displayName ??
                      candidate.nip05Handle ??
                      truncatePubkey(candidate.pubkey)}
                  </span>
                </button>
              ))}
            </div>
          </DialogContent>
        </Dialog>
      ) : null}
    </div>
  );
}
