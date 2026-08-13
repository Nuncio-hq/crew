import * as React from "react";

import {
  getProjectWorktreeEntryByRoot,
  useProjectWorktreeRegistry,
} from "@/features/agents/projectWorktreeRegistryStore";
import { parseProjectThreadContext } from "@/features/messages/lib/projectThreadWorkspace";
import {
  buildProjectThreadBadge,
  type ProjectThreadBadge,
} from "@/features/messages/lib/projectThreadBadge";
import { projectThreadLabel } from "@/features/messages/lib/projectThreadLabel";
import type { TimelineMessage } from "@/features/messages/types";
import { useStableJsonValue } from "@/shared/hooks/useStableReference";

function coworkVersionsHref(repoAddress: string, threadId: string): string {
  // Hash history: colons in 30617 coordinates must stay unencoded so the
  // route param matches `projectMatchesRouteId`.
  return `#/projects/${repoAddress}?thread=${threadId}`;
}

export function useProjectThreadBadge(
  message: TimelineMessage | null | undefined,
): ProjectThreadBadge | null {
  const body = message?.body ?? "";
  const isProjectThread = body.includes("buzz://project-workspace?");
  const context = React.useMemo(
    () =>
      isProjectThread && message
        ? parseProjectThreadContext(message.body)
        : null,
    [isProjectThread, message],
  );
  const repositoryPath = context?.localPath ?? null;
  const { snapshot: registry } = useProjectWorktreeRegistry(repositoryPath);

  const raw = React.useMemo(() => {
    if (!message || !isProjectThread || !context) return null;
    const label = projectThreadLabel(message.body);
    const entry =
      registry.status === "ready" && repositoryPath
        ? getProjectWorktreeEntryByRoot(repositoryPath, message.id)
        : null;
    const fromRegistry = entry ? buildProjectThreadBadge(entry, label) : null;
    if (context.mode === "folder") {
      return {
        label: "cowork",
        branch: "cowork",
        shortBranch: "cowork",
        glyph: "📁" as const,
        mono: false,
        pullRequests: [],
        overflow: 0,
        diff: null,
        openIssues: null,
        href: coworkVersionsHref(context.repoAddress, message.id),
      };
    }
    if (context.ws === "main") {
      return {
        label: "main",
        branch: "main",
        shortBranch: "main",
        glyph: "⌂" as const,
        mono: false,
        pullRequests: fromRegistry?.pullRequests ?? [],
        overflow: fromRegistry?.overflow ?? 0,
        diff: fromRegistry?.diff ?? null,
        openIssues: fromRegistry?.openIssues ?? null,
      };
    }
    if (context.ws === "branch") {
      const name = context.branch ?? "branch";
      return {
        label: name,
        branch: name,
        shortBranch: name,
        glyph: "⎇" as const,
        mono: true,
        pullRequests: fromRegistry?.pullRequests ?? [],
        overflow: fromRegistry?.overflow ?? 0,
        diff: fromRegistry?.diff ?? null,
        openIssues: fromRegistry?.openIssues ?? null,
      };
    }
    if (fromRegistry) {
      return { ...fromRegistry, glyph: "🌿" as const };
    }
    return {
      label,
      branch: context.base ?? "new worktree",
      shortBranch: context.base ?? "new",
      glyph: "🌿" as const,
      mono: label == null,
      pullRequests: [],
      overflow: 0,
      diff: null,
      openIssues: null,
    };
  }, [context, isProjectThread, message, registry, repositoryPath]);

  return useStableJsonValue(raw);
}
