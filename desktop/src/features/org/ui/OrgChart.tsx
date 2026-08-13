import {
  directReports,
  type OrgNode,
  type OrgRoster,
} from "@/features/org/lib/orgRoster";
import { OrgNodeCard } from "@/features/org/ui/OrgNodeCard";
import type { PresenceLookup } from "@/shared/api/types";

function TreeBranch({
  roster,
  manager,
  depth,
  selectedPubkey,
  names,
  avatars,
  presence,
  repoCounts,
  onSelect,
}: {
  roster: OrgRoster;
  manager: string;
  depth: number;
  selectedPubkey: string | null;
  names: Record<string, string>;
  avatars: Record<string, string | null>;
  presence: PresenceLookup;
  repoCounts: Record<string, number>;
  onSelect: (pubkey: string) => void;
}) {
  const reports = directReports(roster, manager);
  if (reports.length === 0) {
    return null;
  }
  return (
    <ul
      className={
        depth === 0
          ? "flex flex-col gap-2"
          : "ml-4 flex flex-col gap-2 border-l border-border/50 pl-3"
      }
    >
      {reports.map((node: OrgNode) => {
        const parent = roster.nodes[node.manager];
        const allocation = parent
          ? node.budget.tokensPerDay / Math.max(parent.budget.tokensPerDay, 1)
          : 1;
        return (
          <li key={node.agentPubkey}>
            <OrgNodeCard
              allocation={allocation}
              avatarUrl={avatars[node.agentPubkey]}
              name={names[node.agentPubkey] ?? node.agentPubkey}
              node={node}
              onSelect={() => onSelect(node.agentPubkey)}
              presence={presence[node.agentPubkey]}
              repoCount={repoCounts[node.agentPubkey] ?? 0}
              selected={selectedPubkey === node.agentPubkey}
            />
            <TreeBranch
              avatars={avatars}
              depth={depth + 1}
              manager={node.agentPubkey}
              names={names}
              onSelect={onSelect}
              presence={presence}
              repoCounts={repoCounts}
              roster={roster}
              selectedPubkey={selectedPubkey}
            />
          </li>
        );
      })}
    </ul>
  );
}

export function OrgChart({
  roster,
  selectedPubkey,
  names,
  avatars,
  presence,
  repoCounts,
  onSelect,
}: {
  roster: OrgRoster;
  selectedPubkey: string | null;
  names: Record<string, string>;
  avatars: Record<string, string | null>;
  presence: PresenceLookup;
  repoCounts: Record<string, number>;
  onSelect: (pubkey: string) => void;
}) {
  return (
    <div className="min-h-0 flex-1 overflow-y-auto p-4" data-testid="org-chart">
      <p className="mb-3 text-2xs uppercase tracking-wide text-muted-foreground">
        Founder
      </p>
      <p className="mb-4 truncate text-sm font-medium">
        {names[roster.founderPubkey] ?? roster.founderPubkey}
      </p>
      <TreeBranch
        avatars={avatars}
        depth={0}
        manager={roster.founderPubkey}
        names={names}
        onSelect={onSelect}
        presence={presence}
        repoCounts={repoCounts}
        roster={roster}
        selectedPubkey={selectedPubkey}
      />
    </div>
  );
}
