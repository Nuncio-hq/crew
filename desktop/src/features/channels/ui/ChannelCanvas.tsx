import { Pencil, Save, X } from "lucide-react";
import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useCommunities } from "@/features/communities/useCommunities";
import { useIdentityQuery } from "@/shared/api/hooks";
import { ChannelRolesDialog } from "./ChannelRolesDialog";
import { useChannelCanvasLive } from "../hooks/useChannelCanvasLive";

import {
  useCanvasQuery,
  useSetCanvasMutation,
} from "@/features/channels/hooks";
import { useChannelNavigation } from "@/shared/context/ChannelNavigationContext";
import { Button } from "@/shared/ui/button";
import { Markdown } from "@/shared/ui/markdown";
import { Textarea } from "@/shared/ui/textarea";
import {
  isRelayUnreachableError,
  RELAY_UNREACHABLE_SHORT,
} from "@/shared/lib/relayError";

type ChannelCanvasProps = {
  channelId: string | null;
  canEdit: boolean;
  isArchived: boolean;
};

export function ChannelCanvas({
  channelId,
  canEdit,
  isArchived,
}: ChannelCanvasProps) {
  const canvasQuery = useCanvasQuery(channelId, channelId !== null);
  const setCanvasMutation = useSetCanvasMutation(channelId);
  const queryClient = useQueryClient();
  const { activeCommunity, reinitKey } = useCommunities();
  const identity = useIdentityQuery();
  const editorScope = `${activeCommunity?.id}:${reinitKey}:${identity.data?.pubkey}:${identity.dataUpdatedAt}:${channelId}`;
  const liveError = useChannelCanvasLive(channelId, editorScope);
  const [rolesScope, setRolesScope] = React.useState<string | null>(null);
  const [savedHead, setSavedHead] = React.useState<{
    scope: string;
    id: string | null;
    observedAt: number;
  } | null>(null);
  const { channels } = useChannelNavigation();
  const channelNames = React.useMemo(
    () => channels.filter((c) => c.channelType !== "dm").map((c) => c.name),
    [channels],
  );
  const [isEditing, setIsEditing] = React.useState(false);
  const [draft, setDraft] = React.useState("");

  const canvasContent = canvasQuery.data?.content ?? null;
  const routing = canvasQuery.data?.routing ?? [];
  const assignments = canvasQuery.data?.assignments ?? [];
  const devMcpGranted = canvasQuery.data?.devMcpGranted;
  const crewParseError = canvasQuery.data?.crewParseError;
  // Defer the single large Markdown parse so opening the canvas commits the
  // surrounding chrome immediately and the heavy render reconciles after.
  const deferredCanvasContent = React.useDeferredValue(canvasContent);

  function handleStartEditing() {
    setDraft(canvasContent ?? "");
    setIsEditing(true);
  }

  function handleCancelEditing() {
    setIsEditing(false);
    setDraft("");
  }

  async function handleSave() {
    await setCanvasMutation.mutateAsync(draft);
    setIsEditing(false);
  }

  if (canvasQuery.isLoading) {
    return <p className="text-sm text-muted-foreground">Loading canvas...</p>;
  }

  if (canvasQuery.error instanceof Error && !canvasQuery.data) {
    return (
      <div className="space-y-2">
        <p className="rounded-xl border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {isRelayUnreachableError(canvasQuery.error)
            ? RELAY_UNREACHABLE_SHORT
            : canvasQuery.error.message}
        </p>
        <Button
          type="button"
          variant="outline"
          onClick={() => {
            void canvasQuery.refetch();
          }}
        >
          Refresh canvas
        </Button>
      </div>
    );
  }

  if (isEditing) {
    return (
      <div className="space-y-3">
        <Textarea
          aria-label="Canvas content"
          className="min-h-48 font-mono text-sm"
          data-testid="channel-canvas-editor"
          disabled={setCanvasMutation.isPending}
          onChange={(event) => setDraft(event.target.value)}
          placeholder="Write your canvas content in Markdown..."
          value={draft}
        />
        <div className="flex gap-2">
          <Button
            data-testid="channel-canvas-save"
            disabled={setCanvasMutation.isPending}
            onClick={() => {
              void handleSave().catch(() => {
                // Error is already surfaced via setCanvasMutation.error
              });
            }}
            size="sm"
            type="button"
          >
            <Save className="h-4 w-4" />
            {setCanvasMutation.isPending ? "Saving..." : "Save canvas"}
          </Button>
          <Button
            data-testid="channel-canvas-cancel"
            disabled={setCanvasMutation.isPending}
            onClick={handleCancelEditing}
            size="sm"
            type="button"
            variant="outline"
          >
            <X className="h-4 w-4" />
            Cancel
          </Button>
        </div>
        {setCanvasMutation.error instanceof Error ? (
          <p className="text-sm text-destructive">
            {setCanvasMutation.error.message}
          </p>
        ) : null}
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {canvasQuery.error instanceof Error ? (
        <p role="alert" className="text-sm text-destructive">
          Canvas refresh failed. The previous version remains visible.
        </p>
      ) : null}
      {liveError ? (
        <p role="status" className="text-sm text-muted-foreground">
          {liveError}
        </p>
      ) : null}
      <Button
        type="button"
        size="sm"
        variant="outline"
        disabled={canvasQuery.isFetching}
        onClick={() => {
          void canvasQuery.refetch();
        }}
      >
        Refresh canvas
      </Button>
      {channelId && canEdit && !isArchived && rolesScope === editorScope ? (
        <ChannelRolesDialog
          key={`${editorScope}:${channelId}`}
          channelId={channelId}
          onClose={() => setRolesScope(null)}
          onApplied={(id) => {
            setSavedHead({
              scope: `${editorScope}:${channelId}`,
              id,
              observedAt: canvasQuery.dataUpdatedAt,
            });
            setRolesScope(null);
            void queryClient.invalidateQueries({
              queryKey: ["channel-canvas", channelId],
            });
          }}
        />
      ) : null}
      {savedHead?.scope === `${editorScope}:${channelId}` ? (
        <p role="status" className="text-sm text-muted-foreground">
          {canvasQuery.dataUpdatedAt > savedHead.observedAt &&
          canvasQuery.data?.eventId !== savedHead.id
            ? "A newer canvas replaced your saved roles."
            : "Roles are current on the relay. Existing sessions need a restart to load the changes."}
        </p>
      ) : null}
      {canvasContent ? (
        <div
          className="rounded-2xl border border-border/70 bg-muted/20 px-4 py-3"
          data-testid="channel-canvas-content"
        >
          <Markdown
            channelNames={channelNames}
            content={deferredCanvasContent ?? ""}
          />
        </div>
      ) : (
        <p className="text-sm text-muted-foreground">
          No canvas set for this channel.
        </p>
      )}
      {crewParseError ? (
        <p
          className="rounded-xl border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive"
          data-testid="channel-canvas-crew-error"
        >
          {crewParseError}
        </p>
      ) : null}
      {routing.length > 0 ? (
        <div
          className="rounded-xl border border-border/70 px-4 py-3"
          data-testid="channel-canvas-routing"
        >
          <p className="text-sm font-medium">Routing presets</p>
          <ul className="mt-2 space-y-1 text-sm">
            {routing.map((preset) => (
              <li key={preset.workType}>
                <span className="font-medium">{preset.workType}</span>
                {" → "}
                <span>{preset.roleLabel}</span>
                {": "}
                {preset.holders.length > 0 ? (
                  preset.holders.join(", ")
                ) : (
                  <span className="text-attention">{preset.unheldMessage}</span>
                )}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
      {assignments.length > 0 ? (
        <div
          className="rounded-xl border border-border/70 px-4 py-3"
          data-testid="channel-canvas-assignments"
        >
          <p className="text-sm font-medium">Channel role assignments</p>
          <ul className="mt-2 space-y-1 text-sm">
            {assignments.map((assignment) => (
              <li key={assignment.agentPubkey}>
                <span className="font-medium">{assignment.roleLabel}</span>
                {" · "}
                <span className="font-mono text-xs">
                  {assignment.agentPubkey}
                </span>
              </li>
            ))}
          </ul>
        </div>
      ) : null}
      {devMcpGranted !== null && devMcpGranted !== undefined ? (
        <p
          className="text-xs text-muted-foreground"
          data-testid="channel-canvas-capability"
        >
          Channel dev-mcp: {devMcpGranted ? "granted" : "denied"}
          {devMcpGranted
            ? null
            : " — this channel's session is also clamped to the engine's read-only mode where the engine honours a session-scoped floor; engines that refuse it fall back to instruction only"}
        </p>
      ) : null}
      {canEdit && !isArchived ? (
        <div className="flex flex-wrap gap-2">
          <Button
            data-testid="channel-canvas-edit"
            onClick={handleStartEditing}
            size="sm"
            type="button"
            variant="outline"
          >
            <Pencil className="h-4 w-4" />
            {canvasContent ? "Edit canvas" : "Create canvas"}
          </Button>
          <Button
            type="button"
            size="sm"
            variant="outline"
            data-testid="channel-manage-roles"
            onClick={() => setRolesScope(editorScope)}
          >
            Manage roles
          </Button>
        </div>
      ) : null}
    </div>
  );
}
