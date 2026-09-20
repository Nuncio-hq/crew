/**
 * #367 — the editable private Wiki task draft and its explicit dispatch.
 *
 * The panel edits a private `wiki:` draft: title, prompt, destination
 * channel, member agent, and which citations cross the boundary. Save writes
 * the local draft store and nothing else — no event, no agent wake. Start is
 * the only boundary: it reserves the durable `thread-handoff` operation
 * (persisted event identity, so retries are the same kickoff) and publishes
 * through the normal channel-root/mention pipeline.
 */
import * as React from "react";

import type { Channel, ChannelMember } from "@/shared/api/types";
import { getChannelMembers, getChannels } from "@/shared/api/tauriChannels";
import { captureOwnerOperationScope } from "@/shared/api/ownerOperations";
import {
  OFFICE_FIELD_BOX_CLASS,
  OFFICE_FIELD_CONTROL_CLASS,
} from "@/shared/layout/officeChrome";
import { truncatePubkey } from "@/shared/lib/pubkey";
import { Button } from "@/shared/ui/button";
import { cn } from "@/shared/lib/cn";

import type { PrivateAskDraftInput } from "@/features/wiki/lib/privateAskDev";
import {
  abandonWikiTaskDispatch,
  buildDispatchInput,
  prepareWikiTaskDispatch,
  reconcileWikiTaskDispatch,
  submitWikiTaskDispatch,
  type WikiTaskDispatchJob,
} from "@/features/wiki/lib/wikiTaskDispatch";
import {
  clearWikiTaskDraft,
  loadWikiTaskDraft,
  newWikiTaskDraftMeta,
  saveWikiTaskDraft,
  wikiTaskDraftKey,
  type WikiTaskDraftMeta,
} from "@/features/wiki/lib/wikiTaskDraft";

type PanelPhase =
  | "editing"
  | "saving"
  | "starting"
  | "publishing"
  | "accepted"
  | "failed";

const MAX_SELECTABLE_REFERENCES = 64;

/**
 * Open the confirmed existing root. The router is a module singleton,
 * imported lazily so a bare-mount test can substitute `onAccepted`.
 */
async function navigateToThread(
  channelId: string,
  rootEventId: string,
): Promise<void> {
  const { router } = await import("@/app/router");
  await router.navigate({
    to: "/channels/$channelId",
    params: { channelId },
    search: { thread: rootEventId } as never,
  });
}

export function WikiTaskDispatchPanel({
  draft,
  onAccepted = navigateToThread,
  onClose,
  projectId,
}: {
  draft: PrivateAskDraftInput;
  /** Called once the relay has accepted the kickoff — opens the new thread. */
  onAccepted?: (channelId: string, rootEventId: string) => void;
  /**
   * Cancel closes the panel and drops only unsaved edits — the saved draft
   * (and every unrelated composer draft) survives.
   */
  onClose: () => void;
  /** Stable project route identity, or "library" on the company surface. */
  projectId: string;
}) {
  const draftKey = React.useMemo(
    () =>
      wikiTaskDraftKey({
        projectId,
        repositoryCoordinate: draft.originCoordinate,
        attemptId: draft.attemptId,
      }),
    [draft.attemptId, draft.originCoordinate, projectId],
  );

  const restored = React.useMemo(() => loadWikiTaskDraft(draftKey), [draftKey]);
  const [meta, setMeta] = React.useState<WikiTaskDraftMeta>(
    () => restored?.meta ?? newWikiTaskDraftMeta(draft),
  );
  const [prompt, setPrompt] = React.useState(
    () => restored?.prompt ?? draft.markdown,
  );
  const [channelId, setChannelId] = React.useState(
    () => restored?.channelId ?? "",
  );
  const [channels, setChannels] = React.useState<Channel[]>([]);
  const [members, setMembers] = React.useState<ChannelMember[]>([]);
  const [membersError, setMembersError] = React.useState<string | null>(null);
  const [phase, setPhase] = React.useState<PanelPhase>("editing");
  const [job, setJob] = React.useState<WikiTaskDispatchJob | null>(null);
  const [note, setNote] = React.useState<string | null>(null);
  const [dirty, setDirty] = React.useState(false);

  React.useEffect(() => {
    let active = true;
    getChannels(null)
      .then((payload) => {
        if (!active) return;
        setChannels(
          (payload.channels ?? []).filter(
            (channel) => channel.isMember !== false && !channel.archivedAt,
          ),
        );
      })
      .catch(() => {
        if (active) setChannels([]);
      });
    return () => {
      active = false;
    };
  }, []);

  React.useEffect(() => {
    if (!channelId) {
      setMembers([]);
      setMembersError(null);
      return;
    }
    let active = true;
    getChannelMembers(channelId)
      .then((list) => {
        if (!active) return;
        setMembers(list);
        setMembersError(null);
      })
      .catch((error: unknown) => {
        if (!active) return;
        setMembers([]);
        setMembersError(error instanceof Error ? error.message : String(error));
      });
    return () => {
      active = false;
    };
  }, [channelId]);

  const agents = React.useMemo(
    () => members.filter((member) => member.isAgent),
    [members],
  );

  const saveDraft = React.useCallback(
    (nextMeta?: WikiTaskDraftMeta) => {
      const record = nextMeta ?? meta;
      saveWikiTaskDraft(draftKey, record, prompt, channelId);
      return record;
    },
    [channelId, draftKey, meta, prompt],
  );

  const onSave = React.useCallback(() => {
    setPhase("saving");
    saveDraft();
    setDirty(false);
    setPhase("editing");
    setNote("Draft saved on this machine — nothing was sent.");
  }, [saveDraft]);

  const toggleReference = React.useCallback((path: string) => {
    setMeta((current) => ({
      ...current,
      references: current.references.map((reference) =>
        reference.path === path
          ? { ...reference, included: !reference.included }
          : reference,
      ),
    }));
    setDirty(true);
  }, []);

  const start = React.useCallback(async () => {
    if (phase === "starting" || phase === "publishing") return;
    setPhase("starting");
    setNote(null);
    try {
      // Mint (or reuse) the dispatch id *and persist it with the draft* before
      // any backend work: a crash/retry of this Start resumes the same
      // operation and the same signed event id.
      const dispatchId = meta.dispatchId ?? crypto.randomUUID();
      const withDispatch = { ...meta, dispatchId };
      setMeta(withDispatch);
      saveDraft(withDispatch);
      const expected = await captureOwnerOperationScope();
      const input = buildDispatchInput({
        agentPubkey: meta.agentPubkey ?? "",
        channelId,
        draft,
        draftKey,
        meta: withDispatch,
        prompt,
      });
      const prepared = await prepareWikiTaskDispatch(expected, input);
      setJob(prepared.value);
      setPhase("publishing");
      const result = await submitWikiTaskDispatch(expected, dispatchId);
      setJob(result.value);
      if (result.value.accepted) {
        setPhase("accepted");
        // The draft's purpose is fulfilled — the operation journal now owns
        // the origin record. Clearing cannot clobber other keys.
        clearWikiTaskDraft(draftKey);
        const root = result.value.rootEventId ?? result.value.eventId;
        if (root) {
          onAccepted(result.value.channelId, root);
        }
      } else {
        setPhase("failed");
        setNote(
          result.value.error ??
            "The relay did not acknowledge this dispatch yet.",
        );
      }
    } catch (error: unknown) {
      setPhase("failed");
      setNote(error instanceof Error ? error.message : String(error));
    }
  }, [channelId, draft, draftKey, meta, onAccepted, phase, prompt, saveDraft]);

  const reconcile = React.useCallback(async () => {
    if (!job) return;
    setPhase("starting");
    try {
      const expected = await captureOwnerOperationScope();
      const result = await reconcileWikiTaskDispatch(expected, job.dispatchId);
      setJob(result.value);
      if (result.value.accepted) {
        setPhase("accepted");
        clearWikiTaskDraft(draftKey);
        const root = result.value.rootEventId ?? result.value.eventId;
        if (root) {
          onAccepted(result.value.channelId, root);
        }
      } else {
        setPhase("failed");
        setNote(
          "The relay does not have this kickoff — retrying is safe; the same signed event is republished.",
        );
      }
    } catch (error: unknown) {
      setPhase("failed");
      setNote(error instanceof Error ? error.message : String(error));
    }
  }, [draftKey, job, onAccepted]);

  const abandon = React.useCallback(async () => {
    if (!job) {
      onClose();
      return;
    }
    try {
      const expected = await captureOwnerOperationScope();
      await abandonWikiTaskDispatch(expected, job.dispatchId, true);
    } catch {
      // A refused abandon still has its durable record; the panel closes and
      // the state stays readable on the next open.
    }
    onClose();
  }, [job, onClose]);

  const chosenAgent = meta.agentPubkey ?? "";
  const busy = phase === "starting" || phase === "publishing";

  return (
    <div
      className="mt-2 rounded-md border border-border bg-muted/10 p-2"
      data-testid="wiki-task-dispatch"
    >
      <div className="mb-1 text-2xs text-muted-foreground">
        Task draft — private until you start the thread.
      </div>
      <label className="block">
        <span className="text-2xs text-muted-foreground">Title</span>
        <input
          aria-label="Task title"
          className={cn(
            OFFICE_FIELD_BOX_CLASS,
            OFFICE_FIELD_CONTROL_CLASS,
            "mt-0.5 h-8 w-full px-2 text-sm",
          )}
          data-testid="wiki-task-title"
          disabled={busy}
          onChange={(event) => {
            setMeta((current) => ({ ...current, title: event.target.value }));
            setDirty(true);
          }}
          value={meta.title}
        />
      </label>
      <label className="mt-1.5 block">
        <span className="text-2xs text-muted-foreground">Prompt</span>
        <textarea
          aria-label="Task prompt"
          className={cn(
            OFFICE_FIELD_BOX_CLASS,
            OFFICE_FIELD_CONTROL_CLASS,
            "mt-0.5 w-full px-2 py-1 text-sm",
          )}
          data-testid="wiki-task-prompt"
          disabled={busy}
          onChange={(event) => {
            setPrompt(event.target.value);
            setDirty(true);
          }}
          rows={4}
          value={prompt}
        />
      </label>
      <div className="mt-1.5 flex items-center gap-2">
        <select
          aria-label="Destination channel"
          className={cn(
            OFFICE_FIELD_BOX_CLASS,
            OFFICE_FIELD_CONTROL_CLASS,
            "h-8 min-w-0 flex-1 px-2 text-2xs",
          )}
          data-testid="wiki-task-channel"
          disabled={busy || channels.length === 0}
          onChange={(event) => {
            setChannelId(event.target.value);
            setDirty(true);
          }}
          value={channelId}
        >
          <option value="">
            {channels.length === 0
              ? "No channel available"
              : "Choose a channel"}
          </option>
          {channels.map((channel) => (
            <option key={channel.id} value={channel.id}>
              #{channel.name}
            </option>
          ))}
        </select>
        <select
          aria-label="Member agent"
          className={cn(
            OFFICE_FIELD_BOX_CLASS,
            OFFICE_FIELD_CONTROL_CLASS,
            "h-8 min-w-0 flex-1 px-2 text-2xs",
          )}
          data-testid="wiki-task-agent"
          disabled={busy || !channelId || agents.length === 0}
          onChange={(event) => {
            setMeta((current) => ({
              ...current,
              agentPubkey: event.target.value || null,
            }));
            setDirty(true);
          }}
          value={chosenAgent}
        >
          <option value="">
            {channelId
              ? agents.length === 0
                ? "No agent is a member of this channel"
                : "Choose an agent"
              : "Choose a channel first"}
          </option>
          {agents.map((member) => (
            <option key={member.pubkey} value={member.pubkey}>
              {member.displayName ?? truncatePubkey(member.pubkey)}
            </option>
          ))}
        </select>
      </div>
      {membersError ? (
        <p
          className="mt-1 text-2xs text-muted-foreground"
          data-testid="wiki-task-members-error"
          role="status"
        >
          Membership could not be read: {membersError}
        </p>
      ) : null}
      {meta.references.length > 0 ? (
        <fieldset className="mt-1.5">
          <legend className="text-2xs text-muted-foreground">
            References shared with the channel
          </legend>
          <ul
            className="mt-0.5 space-y-0.5 text-2xs"
            data-testid="wiki-task-references"
          >
            {meta.references
              .slice(0, MAX_SELECTABLE_REFERENCES)
              .map((reference) => (
                <li key={reference.path}>
                  <label className="flex items-center gap-1.5">
                    <input
                      checked={reference.included}
                      data-testid={`wiki-task-ref-${reference.path}`}
                      disabled={busy}
                      onChange={() => toggleReference(reference.path)}
                      type="checkbox"
                    />
                    <span className="truncate font-mono">
                      {reference.path}:L{reference.startLine}-L
                      {reference.endLine}
                    </span>
                  </label>
                </li>
              ))}
          </ul>
        </fieldset>
      ) : null}
      <div className="mt-2 flex items-center gap-2">
        <Button
          data-testid="wiki-task-start"
          disabled={
            busy ||
            !channelId ||
            !chosenAgent ||
            (meta.title.trim() === "" && prompt.trim() === "")
          }
          onClick={() => void start()}
          size="sm"
          type="button"
        >
          {phase === "publishing" ? "Publishing…" : "Start thread"}
        </Button>
        <Button
          data-testid="wiki-task-save"
          disabled={busy}
          onClick={onSave}
          size="sm"
          type="button"
          variant="secondary"
        >
          Save draft
        </Button>
        <Button
          data-testid="wiki-task-cancel"
          disabled={busy}
          onClick={() => (job ? void abandon() : onClose())}
          size="sm"
          type="button"
          variant="ghost"
        >
          {job ? "Abandon dispatch" : "Cancel"}
        </Button>
        {job && !job.accepted && job.submitAttempts > 0 ? (
          <Button
            data-testid="wiki-task-reconcile"
            disabled={busy}
            onClick={() => void reconcile()}
            size="sm"
            type="button"
            variant="secondary"
          >
            Check status
          </Button>
        ) : null}
        {job && !job.accepted && phase === "failed" ? (
          <Button
            data-testid="wiki-task-retry"
            disabled={busy}
            onClick={() => void start()}
            size="sm"
            type="button"
            variant="secondary"
          >
            Retry
          </Button>
        ) : null}
      </div>
      {dirty ? (
        <p
          className="mt-1 text-2xs text-muted-foreground"
          data-testid="wiki-task-dirty"
        >
          Unsaved changes — Save keeps them on this machine; Start sends only
          what is shown here.
        </p>
      ) : null}
      {note ? (
        <p
          aria-live="polite"
          className="mt-1 text-2xs text-muted-foreground"
          data-testid="wiki-task-note"
          role="status"
        >
          {note}
        </p>
      ) : null}
      {phase === "accepted" ? (
        <p
          aria-live="polite"
          className="mt-1 text-2xs text-muted-foreground"
          data-testid="wiki-task-accepted"
          role="status"
        >
          The thread was created — the new root carries only the reviewed prompt
          and chosen references.
        </p>
      ) : null}
    </div>
  );
}
