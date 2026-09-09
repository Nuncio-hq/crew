import { invoke } from "@tauri-apps/api/core";
import {
  captureOwnerOperationScope,
  listOwnerOperations,
  loadOwnerOperation,
  sameOwnerOperationScope,
  type OwnerOperation,
  type OwnerOperationCreate,
  type OwnerOperationScope,
  type OwnerOperationSummary,
  type ScopedOwnerOperation,
} from "./ownerOperations";

/** Renderer-readable recovery metadata; only the native dispatcher validates authority. */
export type ProjectMetadataAction =
  | {
      type: "attach-repository";
      repository_coordinate: string;
    }
  | {
      type: "unlink-workspace";
      repository_coordinate: string;
    }
  | {
      type: "link-workspace";
      repository_coordinate: string;
      channel_id: string;
      local_path: string;
    };
export type ProjectChannelLinkPayload = {
  version: 1 | 2 | 3 | 4;
  project_coordinate: string;
  channel_id?: string;
  action?: ProjectMetadataAction;
  attempts: number;
  retry_at: number;
  last_error: string | null;
};
export type ProjectChannelLinkOperation =
  OwnerOperation<ProjectChannelLinkPayload>;

export const projectLinkNative = {
  capture: captureOwnerOperationScope,
  list: listOwnerOperations,
  load: loadOwnerOperation<ProjectChannelLinkPayload>,
  prepare: (
    expected: OwnerOperationScope,
    projectCoordinate: string,
    channelId: string,
  ) =>
    invoke<
      ScopedOwnerOperation<OwnerOperationCreate<ProjectChannelLinkPayload>>
    >("project_change_link_prepare", {
      expected,
      projectCoordinate,
      channelId,
    }),
  prepareRepository: (
    expected: OwnerOperationScope,
    projectCoordinate: string,
    repositoryCoordinate: string,
  ) =>
    invoke<
      ScopedOwnerOperation<OwnerOperationCreate<ProjectChannelLinkPayload>>
    >("project_change_attach_repository_prepare", {
      expected,
      projectCoordinate,
      repositoryCoordinate,
    }),
  prepareWorkspace: (
    expected: OwnerOperationScope,
    repositoryCoordinate: string,
    channelId: string,
    localPath: string,
  ) =>
    invoke<
      ScopedOwnerOperation<OwnerOperationCreate<ProjectChannelLinkPayload>>
    >("project_change_link_workspace_prepare", {
      expected,
      repositoryCoordinate,
      channelId,
      localPath,
    }),
  prepareUnlink: (
    expected: OwnerOperationScope,
    repositoryCoordinate: string,
  ) =>
    invoke<
      ScopedOwnerOperation<OwnerOperationCreate<ProjectChannelLinkPayload>>
    >("project_change_unlink_workspace_prepare", {
      expected,
      repositoryCoordinate,
    }),
  dispatch: (
    expected: OwnerOperationScope,
    operation: ProjectChannelLinkOperation,
    explicitRetry: boolean,
  ) =>
    invoke<ScopedOwnerOperation<ProjectChannelLinkOperation>>(
      "project_change_link_dispatch",
      {
        expected,
        id: operation.id,
        revision: operation.revision,
        explicitRetry,
      },
    ),
};
export type ProjectLinkNative = typeof projectLinkNative;

export class ProjectLinkScopeChanged extends Error {
  constructor() {
    super(
      "The active owner or workspace changed. Return to the originating scope to recover.",
    );
  }
}

export async function assertProjectLinkScope(
  expected: OwnerOperationScope,
  native: ProjectLinkNative = projectLinkNative,
) {
  if (!sameOwnerOperationScope(expected, await native.capture()))
    throw new ProjectLinkScopeChanged();
}

function checked<T>(
  result: ScopedOwnerOperation<T>,
  expected: OwnerOperationScope,
): T {
  if (!sameOwnerOperationScope(result.token, expected))
    throw new ProjectLinkScopeChanged();
  return result.value;
}

/** Read recovery on remount; never infer a new operation from a channel name. */
export async function loadProjectChannelLink(
  projectId: string,
  native: ProjectLinkNative = projectLinkNative,
) {
  const token = await native.capture();
  let after: string | null = null;
  // Shared store admits at most 16 unresolved + 100 terminal records per owner.
  for (let page = 0; page < 2; page++) {
    const rows: OwnerOperationSummary[] = checked(
      await native.list(token, after, 100),
      token,
    );
    const row = rows.find(
      (entry) =>
        entry.kind === "project-change" &&
        entry.resource_key === projectId &&
        !entry.reconciled,
    );
    if (row) {
      const operation = checked(
        await native.load(token, row.id, row.revision),
        token,
      );
      if (
        operation.kind !== "project-change" ||
        operation.resource_key !== projectId ||
        !(
          (typeof operation.payload?.channel_id === "string" &&
            operation.payload.channel_id.length <= 36) ||
          ((operation.payload?.action?.type === "attach-repository" ||
            operation.payload?.action?.type === "unlink-workspace") &&
            typeof operation.payload.action.repository_coordinate ===
              "string") ||
          (operation.payload?.action?.type === "link-workspace" &&
            typeof operation.payload.action.repository_coordinate ===
              "string" &&
            typeof operation.payload.action.channel_id === "string" &&
            typeof operation.payload.action.local_path === "string" &&
            operation.payload.action.local_path.length <= 4096)
        ) ||
        (operation.payload.last_error != null &&
          typeof operation.payload.last_error !== "string")
      ) {
        throw new Error(
          "A different Project operation needs recovery before changing this Project.",
        );
      }
      await assertProjectLinkScope(token, native);
      return { token, operation };
    }
    if (rows.length < 100) {
      await assertProjectLinkScope(token, native);
      return { token, operation: null };
    }
    after = rows[rows.length - 1].id;
  }
  throw new Error(
    "Recovery inventory exceeded its expected bound. Retry loading it.",
  );
}

/** Prepare and dispatch one exact repository membership change. */
export async function attachExistingProjectRepository(
  projectId: string,
  repositoryCoordinate: string,
  native: ProjectLinkNative = projectLinkNative,
) {
  const token = await native.capture();
  const created = checked(
    await native.prepareRepository(token, projectId, repositoryCoordinate),
    token,
  );
  const operation = created.operation;
  if (
    operation.kind !== "project-change" ||
    operation.resource_key !== projectId ||
    operation.payload.action?.type !== "attach-repository" ||
    operation.payload.action.repository_coordinate !== repositoryCoordinate
  ) {
    throw new Error(
      "This Project has another pending operation. Recover it before attaching this repository.",
    );
  }
  await assertProjectLinkScope(token, native);
  const result = await native.dispatch(
    token,
    operation,
    created.result === "existing",
  );
  checked(result, token);
  await assertProjectLinkScope(token, native);
  return result;
}

/** Prepare and dispatch one exact repository workspace location change. */
export async function linkProjectWorkspace(
  repositoryCoordinate: string,
  channelId: string,
  localPath: string,
  native: ProjectLinkNative = projectLinkNative,
) {
  const token = await native.capture();
  const created = checked(
    await native.prepareWorkspace(
      token,
      repositoryCoordinate,
      channelId,
      localPath,
    ),
    token,
  );
  const operation = created.operation;
  const action = operation.payload.action;
  if (
    operation.kind !== "project-change" ||
    operation.resource_key !== repositoryCoordinate ||
    action?.type !== "link-workspace" ||
    action.repository_coordinate !== repositoryCoordinate ||
    action.channel_id !== channelId ||
    action.local_path !== localPath
  ) {
    throw new Error(
      "This repository has another pending workspace operation. Recover it before linking a different folder.",
    );
  }
  await assertProjectLinkScope(token, native);
  const result = await native.dispatch(
    token,
    operation,
    created.result === "existing",
  );
  checked(result, token);
  await assertProjectLinkScope(token, native);
  return result;
}

/** Retry a persisted workspace link without preparing a successor event. */
export async function retryProjectWorkspaceLink(
  token: OwnerOperationScope,
  operation: ProjectChannelLinkOperation,
  repositoryCoordinate: string,
  channelId: string,
  localPath: string,
  native: ProjectLinkNative = projectLinkNative,
) {
  const action = operation.payload.action;
  if (
    action?.type !== "link-workspace" ||
    action.repository_coordinate !== repositoryCoordinate ||
    action.channel_id !== channelId ||
    action.local_path !== localPath
  ) {
    throw new Error("The pending workspace link targets another operation.");
  }
  return retryProjectChannelLink(token, operation, native);
}

/** Prepare durably, then invoke only the opaque native record ID and revision. */
export async function linkExistingProjectChannel(
  projectId: string,
  channelId: string,
  native: ProjectLinkNative = projectLinkNative,
) {
  const token = await native.capture();
  const created = checked(
    await native.prepare(token, projectId, channelId),
    token,
  );
  const operation = created.operation;
  if (
    operation.kind !== "project-change" ||
    operation.resource_key !== projectId ||
    operation.payload.channel_id !== channelId
  ) {
    throw new Error(
      "This Project has another pending operation. Recover it before linking this channel.",
    );
  }
  await assertProjectLinkScope(token, native);
  const result = await native.dispatch(
    token,
    operation,
    created.result === "existing",
  );
  checked(result, token);
  await assertProjectLinkScope(token, native);
  return result;
}

/** Explicit retry preserves the native record's channel, coordinate and signed event. */
export async function retryProjectChannelLink(
  token: OwnerOperationScope,
  operation: ProjectChannelLinkOperation,
  native: ProjectLinkNative = projectLinkNative,
) {
  await assertProjectLinkScope(token, native);
  const result = await native.dispatch(token, operation, true);
  checked(result, token);
  await assertProjectLinkScope(token, native);
  return result;
}

/** Retry a repository attachment without preparing a successor intent. */
export async function retryProjectRepositoryAttachment(
  token: OwnerOperationScope,
  operation: ProjectChannelLinkOperation,
  repositoryCoordinate: string,
  native: ProjectLinkNative = projectLinkNative,
) {
  if (
    operation.payload.action?.type !== "attach-repository" ||
    operation.payload.action.repository_coordinate !== repositoryCoordinate
  ) {
    throw new Error(
      "The pending Project operation targets another repository.",
    );
  }
  return retryProjectChannelLink(token, operation, native);
}

/** Prepare and dispatch one exact repository workspace unlink. */
export async function unlinkProjectWorkspace(
  repositoryCoordinate: string,
  native: ProjectLinkNative = projectLinkNative,
) {
  const token = await native.capture();
  const created = checked(
    await native.prepareUnlink(token, repositoryCoordinate),
    token,
  );
  const operation = created.operation;
  if (
    operation.kind !== "project-change" ||
    operation.resource_key !== repositoryCoordinate ||
    operation.payload.action?.type !== "unlink-workspace" ||
    operation.payload.action.repository_coordinate !== repositoryCoordinate
  ) {
    throw new Error(
      "This repository has another pending operation. Recover it before unlinking its workspace.",
    );
  }
  await assertProjectLinkScope(token, native);
  const result = await native.dispatch(
    token,
    operation,
    created.result === "existing",
  );
  checked(result, token);
  await assertProjectLinkScope(token, native);
  return result;
}

/** Retry the persisted unlink envelope without preparing a successor event. */
export async function retryProjectWorkspaceUnlink(
  token: OwnerOperationScope,
  operation: ProjectChannelLinkOperation,
  repositoryCoordinate: string,
  native: ProjectLinkNative = projectLinkNative,
) {
  if (
    operation.payload.action?.type !== "unlink-workspace" ||
    operation.payload.action.repository_coordinate !== repositoryCoordinate
  ) {
    throw new Error("The pending workspace unlink targets another repository.");
  }
  return retryProjectChannelLink(token, operation, native);
}
