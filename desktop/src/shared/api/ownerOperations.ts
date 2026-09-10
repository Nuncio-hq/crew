import { invoke } from "@tauri-apps/api/core";

/** Native capture token; owner/community parameters never grant authority. */
export type OwnerOperationScope = {
  scope: { owner: string; community: string };
  workspace_generation: number;
  identity_generation: number;
};
export type OwnerOperationKind =
  | "wiki-publication"
  | "project-change"
  | "thread-handoff"
  | "channel-crew-config";
export type OwnerOperationStatus =
  | "preparing"
  | "pending"
  | "reconciling"
  | "failed"
  | "complete"
  | "canceled"
  | "superseded";
export type OwnerOperation<T> = {
  version: number;
  scope: OwnerOperationScope["scope"];
  id: string;
  kind: OwnerOperationKind;
  resource_key: string;
  revision: number;
  created_at: number;
  updated_at: number;
  status: OwnerOperationStatus;
  reconciled: boolean;
  payload: T;
};
export type OwnerOperationSummary = Pick<
  OwnerOperation<unknown>,
  | "id"
  | "kind"
  | "resource_key"
  | "revision"
  | "status"
  | "reconciled"
  | "updated_at"
>;
/** Check the token against current UI scope before applying queued results. */
export type ScopedOwnerOperation<T> = {
  token: OwnerOperationScope;
  value: T;
};
export type OwnerOperationCreate<T> = {
  result: "created" | "existing";
  operation: OwnerOperation<T>;
};

export function sameOwnerOperationScope(
  left: OwnerOperationScope,
  right: OwnerOperationScope,
): boolean {
  return (
    left.scope.owner === right.scope.owner &&
    left.scope.community === right.scope.community &&
    left.workspace_generation === right.workspace_generation &&
    left.identity_generation === right.identity_generation
  );
}

export function captureOwnerOperationScope(): Promise<OwnerOperationScope> {
  return invoke("owner_operation_scope");
}

/** Domain validates intent before create and before any signing or sending. */
export function createOwnerOperation<T>(
  expected: OwnerOperationScope,
  operation: Pick<
    OwnerOperation<T>,
    "id" | "kind" | "resource_key" | "payload"
  >,
): Promise<ScopedOwnerOperation<OwnerOperationCreate<T>>> {
  return invoke("owner_operation_create", { expected, operation });
}

export function loadOwnerOperation<T>(
  expected: OwnerOperationScope,
  id: string,
  revision: number | null = null,
): Promise<ScopedOwnerOperation<OwnerOperation<T>>> {
  return invoke("owner_operation_load", { expected, id, revision });
}

export function listOwnerOperations(
  expected: OwnerOperationScope,
  afterId: string | null = null,
  limit = 100,
): Promise<ScopedOwnerOperation<OwnerOperationSummary[]>> {
  return invoke("owner_operation_list", { expected, afterId, limit });
}

/** A resource claim is not a worker lease; the domain persists lease/retry state. */
export function updateOwnerOperation<T>(
  expected: OwnerOperationScope,
  id: string,
  revision: number,
  update: Pick<OwnerOperation<T>, "status" | "reconciled" | "payload">,
): Promise<ScopedOwnerOperation<OwnerOperation<T>>> {
  return invoke("owner_operation_update", { expected, id, revision, update });
}

/** Domain must reconcile every external side effect before marking/removing. */
export function removeOwnerOperation(
  expected: OwnerOperationScope,
  id: string,
  revision: number,
): Promise<ScopedOwnerOperation<null>> {
  return invoke("owner_operation_remove", { expected, id, revision });
}
