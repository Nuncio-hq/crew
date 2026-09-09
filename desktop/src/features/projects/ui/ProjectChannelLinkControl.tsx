import * as React from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { projectsQueryKey } from "@/features/projects/hooks";
import type { Project } from "@/features/projects/projectModels";
import { listProjectBoundChannels } from "@/features/projects/lib/projectRelatedChannels";
import { useIdentityQuery } from "@/shared/api/hooks";
import {
  assertProjectLinkScope,
  linkExistingProjectChannel,
  loadProjectChannelLink,
  ProjectLinkScopeChanged,
  retryProjectChannelLink,
} from "@/shared/api/projectChannelLink";
import type { Channel } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/shared/ui/dialog";

/** A native journal-backed action; query projections never authorize its writes. */
export function ProjectChannelLinkControl({
  project,
  channels,
}: {
  project: Project;
  channels: readonly Channel[];
}) {
  const identity = useIdentityQuery();
  return (
    <ScopedProjectChannelLinkControl
      key={`${project.id}:${identity.data?.pubkey ?? "pending"}`}
      project={project}
      channels={channels}
      owner={identity.data?.pubkey ?? null}
    />
  );
}

function ScopedProjectChannelLinkControl({
  project,
  channels,
  owner,
}: {
  project: Project;
  channels: readonly Channel[];
  owner: string | null;
}) {
  const [open, setOpen] = React.useState(false);
  const [channelId, setChannelId] = React.useState("");
  const selectId = React.useId();
  const queryClient = useQueryClient();
  const recovery = useQuery({
    queryKey: ["project-channel-link-recovery", owner, project.id],
    queryFn: () => loadProjectChannelLink(project.id),
    enabled: Boolean(owner),
    retry: false,
    staleTime: 0,
  });
  const pending = recovery.data?.operation;
  const bound = new Set(
    listProjectBoundChannels(project).map((binding) => binding.channelId),
  );
  const candidates = channels.filter(
    (channel) =>
      channel.isMember &&
      channel.channelType === "stream" &&
      !channel.archivedAt &&
      !bound.has(channel.id),
  );
  const mutation = useMutation({
    mutationFn: async (resume: boolean) => {
      if (resume) {
        if (!recovery.data?.operation)
          throw new Error("Reload this Project's pending operation first.");
        return retryProjectChannelLink(
          recovery.data.token,
          recovery.data.operation,
        );
      }
      return linkExistingProjectChannel(project.id, channelId);
    },
    onSuccess: async (result) => {
      await assertProjectLinkScope(result.token);
      if (result.value.reconciled) {
        void queryClient.invalidateQueries({ queryKey: projectsQueryKey });
        setChannelId("");
        if (result.value.status === "complete") setOpen(false);
      }
      void recovery.refetch();
    },
    onError: () => {
      void recovery.refetch();
    },
  });
  const busy = mutation.isPending || recovery.isFetching;
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button size="sm" variant="outline">
          {pending ? "Recover channel link" : "Link existing channel"}
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Link a channel to {project.name}</DialogTitle>
        </DialogHeader>
        <p className="text-sm text-muted-foreground">
          Choose a channel the Project owner has joined. Linking keeps its
          members, roles and conversations unchanged.
        </p>
        {recovery.isError ? (
          <div role="status" className="space-y-2 text-sm">
            <p>Could not load pending Project operations.</p>
            <Button
              onClick={() => void recovery.refetch()}
              disabled={busy}
              variant="outline"
            >
              Retry loading
            </Button>
          </div>
        ) : null}
        {pending ? (
          <div className="space-y-3 rounded-lg border p-3">
            <p className="text-sm">
              Pending channel:{" "}
              <code className="break-all">
                {pending.payload.channel_id ?? "another Project operation"}
              </code>
            </p>
            <p role="status" className="text-sm text-muted-foreground">
              {pending.payload.last_error ||
                "This operation needs confirmation from the relay."}
            </p>
            <Button onClick={() => mutation.mutate(true)} disabled={busy}>
              Retry this operation
            </Button>
          </div>
        ) : (
          <form
            className="space-y-3"
            onSubmit={(event) => {
              event.preventDefault();
              if (!busy && channelId && !recovery.isError)
                mutation.mutate(false);
            }}
          >
            <label htmlFor={selectId} className="block text-sm font-medium">
              Channel
            </label>
            <select
              id={selectId}
              value={channelId}
              onChange={(event) => setChannelId(event.target.value)}
              disabled={busy || recovery.isError}
              required
              className="w-full rounded-md border bg-background px-3 py-2 text-sm"
            >
              <option value="">Select a channel</option>
              {candidates.map((channel) => (
                <option key={channel.id} value={channel.id}>
                  {channel.name}
                </option>
              ))}
            </select>
            {!candidates.length ? (
              <p className="text-sm text-muted-foreground">
                No unlinked stream channels are available.
              </p>
            ) : null}
            <Button
              type="submit"
              disabled={busy || recovery.isError || !channelId}
            >
              Link channel
            </Button>
          </form>
        )}
        {mutation.data?.value.status === "superseded" &&
        mutation.data.value.reconciled ? (
          <p role="status" className="text-sm text-muted-foreground">
            The Project changed. Review its refreshed details and choose a
            channel to prepare a new link.
          </p>
        ) : null}
        {mutation.error &&
        !(mutation.error instanceof ProjectLinkScopeChanged) ? (
          <p role="alert" className="text-sm text-destructive">
            {mutation.error.message}
          </p>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
