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
    if (!message || !isProjectThread || !repositoryPath) return null;
    if (registry.status !== "ready") return null;
    const entry = getProjectWorktreeEntryByRoot(repositoryPath, message.id);
    if (!entry?.rootEventId) return null;
    const label = projectThreadLabel(message.body);
    return buildProjectThreadBadge(entry, label);
  }, [isProjectThread, message, registry, repositoryPath]);

  return useStableJsonValue(raw);
}
