import * as React from "react";
import { getCanvas } from "@/shared/api/canvas";
import { listRelayAgents } from "@/shared/api/tauri";
import {
  captureOwnerOperationScope,
  sameOwnerOperationScope,
  type OwnerOperationScope,
} from "@/shared/api/ownerOperations";
import {
  discardChannelCrewOperation,
  getChannelCrewOperation,
  listChannelCrewOperations,
  retryChannelCrewConfig,
  saveChannelCrewConfig,
  type CrewSaveProgress,
} from "@/shared/api/channelCrewConfig";
import type { CanvasResponse, RelayAgent } from "@/shared/api/types";
import {
  createRoleDraft,
  createRoleDraftFromSubmitted,
  serializeRoleDraft,
  type ChannelRoleDraft,
} from "../lib/channelRoleDraft";

type Snapshot = {
  scope: OwnerOperationScope;
  canvas: CanvasResponse;
  members: RelayAgent[];
};

function recoveryPriority(progress: CrewSaveProgress): number {
  if (!progress.reconciled) return 0;
  return progress.outcome === "superseded" ? 1 : 2;
}

/** Choose one recovery row without letting terminal history hide live work. */
function selectRecoveryProgress(
  values: readonly CrewSaveProgress[],
): CrewSaveProgress | undefined {
  return [...values].sort(
    (left, right) =>
      recoveryPriority(left) - recoveryPriority(right) ||
      (left.operation_id < right.operation_id
        ? -1
        : left.operation_id > right.operation_id
          ? 1
          : 0),
  )[0];
}

/** Captures both sides of unscoped legacy reads before exposing a draft. */
export async function loadRoleSnapshot(channelId: string): Promise<Snapshot> {
  const scope = await captureOwnerOperationScope();
  const [canvas, agents] = await Promise.all([
    getCanvas(channelId),
    listRelayAgents(),
  ]);
  const after = await captureOwnerOperationScope();
  if (!sameOwnerOperationScope(scope, after))
    throw new Error("Community or identity changed. Reopen Manage roles.");
  return {
    scope,
    canvas,
    members: agents.filter((agent) => agent.channelIds.includes(channelId)),
  };
}

export function useChannelRoleEditor(
  channelId: string,
  onApplied: (eventId: string | null) => void,
) {
  const [snapshot, setSnapshot] = React.useState<Snapshot | null>(null);
  const [draft, setDraft] = React.useState<ChannelRoleDraft | null>(null);
  const [review, setReview] = React.useState<Snapshot | null>(null);
  const [operation, setOperation] = React.useState<string | null>(null);
  const [progress, setProgress] = React.useState<CrewSaveProgress | null>(null);
  const [busy, setBusy] = React.useState(true);
  const [error, setError] = React.useState<string | null>(null);
  const [conflict, setConflict] = React.useState(false);
  const ticket = React.useRef(0);
  const statusRequest = React.useRef(0);
  const activeOperation = React.useRef<string | null>(null);
  const actionClaimed = React.useRef(false);
  const applied = React.useRef(onApplied);
  applied.current = onApplied;

  const verify = React.useCallback(
    async (scope: OwnerOperationScope, run: number) => {
      const current = await captureOwnerOperationScope();
      if (run !== ticket.current) return false;
      if (!sameOwnerOperationScope(current, scope))
        throw new Error("Community or identity changed. Reopen Manage roles.");
      return true;
    },
    [],
  );

  React.useEffect(() => {
    const run = ++ticket.current;
    void (async () => {
      try {
        const loaded = await loadRoleSnapshot(channelId);
        const pending = await listChannelCrewOperations(
          loaded.scope,
          channelId,
        );
        if (!(await verify(loaded.scope, run))) return;
        if (!sameOwnerOperationScope(pending.token, loaded.scope))
          throw new Error("Recovery belongs to another community or identity.");
        setSnapshot(loaded);
        const currentMembers = loaded.members.map((member) => member.pubkey);
        const recovery = selectRecoveryProgress(pending.value);
        if (recovery) {
          setDraft(
            recovery.draft
              ? createRoleDraftFromSubmitted(
                  loaded.canvas,
                  currentMembers,
                  recovery.draft,
                )
              : createRoleDraft(loaded.canvas, currentMembers),
          );
          activeOperation.current = recovery.operation_id;
          setOperation(recovery.operation_id);
          setProgress(recovery);
          setConflict(recovery.outcome === "superseded");
        } else {
          setDraft(createRoleDraft(loaded.canvas, currentMembers));
        }
      } catch (cause) {
        if (run === ticket.current) setError(String(cause));
      } finally {
        if (run === ticket.current) setBusy(false);
      }
    })();
    return () => {
      ticket.current++;
    };
  }, [channelId, verify]);

  const acceptProgress = React.useCallback((value: CrewSaveProgress) => {
    activeOperation.current = value.operation_id;
    setProgress(value);
    setOperation(value.operation_id);
    if (
      value.outcome === "applied" &&
      value.current_event_id === value.canvas_event_id
    )
      applied.current(value.canvas_event_id);
    if (value.outcome === "superseded") setConflict(true);
  }, []);

  const refresh = React.useCallback(
    async (manual = false) => {
      if (
        !snapshot ||
        !operation ||
        operation !== activeOperation.current ||
        actionClaimed.current
      )
        return;
      const run = ticket.current;
      const request = ++statusRequest.current;
      const isCurrent = () =>
        run === ticket.current &&
        request === statusRequest.current &&
        operation === activeOperation.current;
      if (manual) {
        actionClaimed.current = true;
        setBusy(true);
      }
      try {
        if (!(await verify(snapshot.scope, run)) || !isCurrent()) return;
        const result = manual
          ? await retryChannelCrewConfig(snapshot.scope, operation)
          : await getChannelCrewOperation(snapshot.scope, operation);
        if (!(await verify(snapshot.scope, run)) || !isCurrent()) return;
        if (!sameOwnerOperationScope(result.token, snapshot.scope))
          throw new Error("Recovery scope changed.");
        if (result.value.operation_id !== operation)
          throw new Error("Recovery returned another operation.");
        setError(null);
        acceptProgress(result.value);
      } catch (cause) {
        if (isCurrent()) setError(String(cause));
      } finally {
        if (manual) {
          actionClaimed.current = false;
          if (run === ticket.current) setBusy(false);
        }
      }
    },
    [snapshot, operation, verify, acceptProgress],
  );

  React.useEffect(() => {
    if (
      !operation ||
      progress?.outcome === "superseded" ||
      progress?.manual_retry_required
    )
      return;
    let polls = 0;
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      await refresh();
      if (active && ++polls < 60)
        timer = setTimeout(() => {
          void poll();
        }, 5000);
    };
    timer = setTimeout(() => {
      void poll();
    }, 1000);
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [operation, progress?.outcome, progress?.manual_retry_required, refresh]);

  async function save() {
    if (
      !snapshot ||
      !draft ||
      operation ||
      activeOperation.current ||
      busy ||
      conflict ||
      actionClaimed.current
    )
      return;
    actionClaimed.current = true;
    statusRequest.current++;
    const run = ticket.current;
    setBusy(true);
    setError(null);
    try {
      if (!(await verify(snapshot.scope, run))) return;
      const result = await saveChannelCrewConfig(
        snapshot.scope,
        channelId,
        snapshot.canvas.eventId,
        serializeRoleDraft(draft),
      );
      if (!(await verify(snapshot.scope, run))) return;
      if (!sameOwnerOperationScope(result.token, snapshot.scope))
        throw new Error("Save scope changed.");
      const value = result.value;
      if (value.result === "saved") acceptProgress(value.progress);
      else if (value.result === "recovery_pending") {
        activeOperation.current = value.operation_id;
        setOperation(value.operation_id);
      } else if (value.result === "unchanged")
        applied.current(value.current_event_id);
      else {
        setConflict(true);
        setError(
          value.result === "review_required"
            ? "This canvas needs review in Edit canvas before roles can be saved."
            : "The canvas changed. Load the latest version to review it. Your draft is retained.",
        );
      }
    } catch (cause) {
      if (run === ticket.current) setError(String(cause));
    } finally {
      actionClaimed.current = false;
      if (run === ticket.current) setBusy(false);
    }
  }

  async function loadLatest() {
    if (!snapshot || actionClaimed.current) return;
    actionClaimed.current = true;
    const run = ticket.current;
    setBusy(true);
    try {
      const latest = await loadRoleSnapshot(channelId);
      if (!(await verify(snapshot.scope, run))) return;
      if (!sameOwnerOperationScope(snapshot.scope, latest.scope))
        throw new Error("Review scope changed.");
      setReview(latest);
    } catch (cause) {
      if (run === ticket.current) setError(String(cause));
    } finally {
      actionClaimed.current = false;
      if (run === ticket.current) setBusy(false);
    }
  }

  async function replaceDraft() {
    if (
      !review ||
      actionClaimed.current ||
      (operation && progress?.outcome !== "superseded")
    )
      return;
    actionClaimed.current = true;
    statusRequest.current++;
    const run = ticket.current;
    setBusy(true);
    setError(null);
    try {
      if (!(await verify(review.scope, run))) return;
      if (operation) await discardChannelCrewOperation(review.scope, operation);
      if (!(await verify(review.scope, run))) return;
      setSnapshot(review);
      setDraft(
        createRoleDraft(
          review.canvas,
          review.members.map((member) => member.pubkey),
        ),
      );
      setReview(null);
      setConflict(false);
      setOperation(null);
      setProgress(null);
      activeOperation.current = null;
    } catch (cause) {
      if (run === ticket.current) setError(String(cause));
    } finally {
      actionClaimed.current = false;
      if (run === ticket.current) setBusy(false);
    }
  }

  return {
    snapshot,
    draft,
    setDraft,
    review,
    operation,
    progress,
    busy,
    error,
    conflict,
    save,
    refresh,
    loadLatest,
    replaceDraft,
  };
}
