import { Avatar, AvatarFallback, AvatarImage } from "@/shared/ui/avatar";
import { truncatePubkey } from "@/shared/lib/pubkey";
import type { PresenceStatus } from "@/shared/api/types";
import type { OrgNode } from "@/features/org/lib/orgRoster";

function presenceGlyph(status: PresenceStatus | undefined): string {
  if (status === "online") return "🟢";
  if (status === "away") return "🟡";
  return "🌙";
}

export function OrgNodeCard({
  node,
  name,
  avatarUrl,
  presence,
  repoCount,
  allocation,
  selected,
  onSelect,
}: {
  node: OrgNode;
  name: string;
  avatarUrl?: string | null;
  presence?: PresenceStatus;
  repoCount: number;
  allocation: number;
  selected: boolean;
  onSelect: () => void;
}) {
  const fill = Math.max(0, Math.min(1, allocation));
  const amber = fill >= 0.8;
  const initial = name.trim().slice(0, 1).toUpperCase() || "?";
  return (
    <button
      className={`flex w-full min-w-0 items-start gap-2 rounded-lg border px-2 py-1.5 text-left ${
        selected ? "border-primary bg-primary/10" : "border-border/60 bg-card"
      }`}
      data-testid={`org-node-${node.agentPubkey}`}
      onClick={onSelect}
      type="button"
    >
      <Avatar className="h-8 w-8">
        {avatarUrl ? <AvatarImage alt="" src={avatarUrl} /> : null}
        <AvatarFallback className="text-2xs">{initial}</AvatarFallback>
      </Avatar>
      <span className="min-w-0 flex-1">
        <span className="flex items-center gap-1">
          <span className="truncate text-sm font-medium">{name}</span>
          <span aria-hidden className="text-2xs">
            {presenceGlyph(presence)}
          </span>
        </span>
        <span className="block truncate text-2xs text-muted-foreground">
          {node.domain}
        </span>
        <span className="mt-1 block h-1.5 overflow-hidden rounded-full bg-muted">
          <span
            className={`block h-full ${amber ? "bg-attention" : "bg-primary/70"}`}
            style={{ width: `${Math.round(fill * 100)}%` }}
          />
        </span>
        <span className="mt-0.5 block text-3xs text-muted-foreground">
          {node.budget.tokensPerDay} tok/day · cap {node.budget.openWorkCap}
          {repoCount > 0 ? ` · 📁 ${repoCount} repos` : ""}
        </span>
        <span className="sr-only">{truncatePubkey(node.agentPubkey)}</span>
      </span>
    </button>
  );
}
