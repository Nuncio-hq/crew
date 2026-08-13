import type { RelayEvent } from "@/shared/api/types";

import {
  projectLocalWorkspaceFromEvent,
  readCanonicalProjectChannel,
  readCrewWorkspaceMode,
} from "./project-local-workspace";
import { effectiveCloneUrls } from "./projectCloneUrl";

function cloneUrls(event: RelayEvent) {
  const tag = event.tags.find((candidate) => candidate[0] === "clone");
  return tag ? tag.slice(1) : [];
}

export function projectWorkspaceReadFields(
  event: RelayEvent,
  relayOrigin?: string | null,
) {
  const workspace = projectLocalWorkspaceFromEvent(event);
  const canonicalChannel = readCanonicalProjectChannel(event.tags);
  const localWorkspacePath =
    workspace.localWorkspace.status === "linked"
      ? workspace.localWorkspace.path
      : null;
  const explicitCloneUrls = cloneUrls(event);
  const workspaceMode = readCrewWorkspaceMode(event.tags);

  return {
    localWorkspacePath,
    localWorkspaceStatus: workspace.localWorkspace.status,
    workspaceMode,
    cloneUrls:
      workspace.localWorkspace.status === "invalid" ||
      (localWorkspacePath && explicitCloneUrls.length === 0)
        ? []
        : effectiveCloneUrls(
            explicitCloneUrls,
            relayOrigin,
            event.pubkey,
            event.tags.find((tag) => tag[0] === "d")?.[1] ?? event.id,
          ),
    canonicalChannel,
  };
}
