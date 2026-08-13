import * as React from "react";
import { Network } from "lucide-react";

import { useOrgRosterQuery } from "@/features/org/hooks/useOrgRosterQuery";
import { usePublishOrgRoster } from "@/features/org/hooks/usePublishOrgRoster";
import {
  displayNameForPubkey,
  portfolioCountForAgent,
} from "@/features/org/lib/orgRoster";
import { OrgChart } from "@/features/org/ui/OrgChart";
import { OrgDrillPanel } from "@/features/org/ui/OrgDrillPanel";
import { OrgRosterEditor } from "@/features/org/ui/OrgRosterEditor";
import { useManagedAgentsQuery } from "@/features/agents/hooks";
import { useRelayMembersQuery } from "@/features/community-members/hooks";
import { usePresenceQuery } from "@/features/presence/hooks";
import { useUsersBatchQuery } from "@/features/profile/hooks";
import { useProjectsQuery } from "@/features/projects/hooks";
import { useIdentityQuery } from "@/shared/api/hooks";
import { TopChromeInsetHeader } from "@/shared/layout/TopChromeInsetHeader";
import { Button } from "@/shared/ui/button";

export function OrgScreen() {
  const rosterQuery = useOrgRosterQuery();
  const identity = useIdentityQuery();
  const membersQuery = useRelayMembersQuery();
  const agentsQuery = useManagedAgentsQuery();
  const projectsQuery = useProjectsQuery();
  const publish = usePublishOrgRoster();
  const [selected, setSelected] = React.useState<string | null>(null);
  const [editorOpen, setEditorOpen] = React.useState(false);

  const viewerPubkey = identity.data?.pubkey?.toLowerCase() ?? "";
  const isFounder = (membersQuery.data ?? []).some(
    (member) =>
      member.pubkey.toLowerCase() === viewerPubkey && member.role === "owner",
  );
  const roster = rosterQuery.data;
  const nodePubkeys = roster ? Object.keys(roster.nodes) : [];
  const lookupPubkeys = React.useMemo(() => {
    const keys = new Set<string>(nodePubkeys);
    if (roster) {
      keys.add(roster.founderPubkey);
    }
    for (const agent of agentsQuery.data ?? []) {
      keys.add(agent.pubkey.toLowerCase());
    }
    return [...keys];
  }, [agentsQuery.data, nodePubkeys, roster]);
  const profilesQuery = useUsersBatchQuery(lookupPubkeys);
  const presenceQuery = usePresenceQuery(lookupPubkeys);

  const names = React.useMemo(() => {
    const next: Record<string, string> = {};
    const profiles = profilesQuery.data?.profiles ?? {};
    for (const pubkey of lookupPubkeys) {
      next[pubkey] = displayNameForPubkey(pubkey, profiles);
    }
    if (viewerPubkey) {
      next[viewerPubkey] = next[viewerPubkey] || "You";
    }
    return next;
  }, [lookupPubkeys, profilesQuery.data?.profiles, viewerPubkey]);

  const avatars = React.useMemo(() => {
    const next: Record<string, string | null> = {};
    const profiles = profilesQuery.data?.profiles ?? {};
    for (const pubkey of lookupPubkeys) {
      next[pubkey] = profiles[pubkey]?.avatarUrl ?? null;
    }
    return next;
  }, [lookupPubkeys, profilesQuery.data?.profiles]);

  const repoCounts = React.useMemo(() => {
    const repositories = (projectsQuery.data ?? []).flatMap(
      (project) => project.repositories,
    );
    const next: Record<string, number> = {};
    for (const pubkey of nodePubkeys) {
      next[pubkey] = portfolioCountForAgent(pubkey, repositories);
    }
    return next;
  }, [nodePubkeys, projectsQuery.data]);

  const memberPubkeys = React.useMemo(() => {
    const keys = new Set<string>();
    for (const member of membersQuery.data ?? []) {
      keys.add(member.pubkey.toLowerCase());
    }
    for (const agent of agentsQuery.data ?? []) {
      keys.add(agent.pubkey.toLowerCase());
    }
    return [...keys];
  }, [agentsQuery.data, membersQuery.data]);

  const selectedNode = selected && roster ? roster.nodes[selected] : null;
  const founderPubkey = roster?.founderPubkey ?? viewerPubkey;

  return (
    <div
      className="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
      data-testid="org-view"
    >
      <TopChromeInsetHeader>
        <header className="flex h-9 items-center gap-2 px-5">
          <Network className="h-4 w-4" />
          <h1 className="text-base font-semibold">Org</h1>
          <span className="text-2xs text-muted-foreground">
            Projection of the founder-signed roster
          </span>
          <span className="flex-1" />
          {isFounder ? (
            <Button
              data-testid="org-edit-roster"
              onClick={() => setEditorOpen(true)}
              size="sm"
              type="button"
              variant="outline"
            >
              Edit roster
            </Button>
          ) : null}
        </header>
      </TopChromeInsetHeader>
      <div className="flex min-h-0 flex-1">
        {roster && Object.keys(roster.nodes).length > 0 ? (
          <OrgChart
            avatars={avatars}
            names={names}
            onSelect={setSelected}
            presence={presenceQuery.data ?? {}}
            repoCounts={repoCounts}
            roster={roster}
            selectedPubkey={selected}
          />
        ) : (
          <div className="flex flex-1 items-center justify-center p-8 text-sm text-muted-foreground">
            {isFounder
              ? "No roster yet. Appoint officers, then publish."
              : "The founder has not published an org roster."}
          </div>
        )}
        {selectedNode ? (
          <OrgDrillPanel
            name={names[selectedNode.agentPubkey] ?? selectedNode.agentPubkey}
            node={selectedNode}
            onClose={() => setSelected(null)}
          />
        ) : null}
      </div>
      {isFounder ? (
        <OrgRosterEditor
          founderPubkey={founderPubkey}
          memberPubkeys={memberPubkeys}
          names={names}
          onOpenChange={setEditorOpen}
          onPublish={async (next) => {
            await publish.mutateAsync(next);
            setEditorOpen(false);
          }}
          open={editorOpen}
          publishing={publish.isPending}
          roster={roster ?? null}
        />
      ) : null}
    </div>
  );
}
