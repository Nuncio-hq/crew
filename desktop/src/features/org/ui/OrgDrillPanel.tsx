import { truncatePubkey } from "@/shared/lib/pubkey";
import type { OrgNode } from "@/features/org/lib/orgRoster";

export function OrgDrillPanel({
  node,
  name,
  onClose,
}: {
  node: OrgNode;
  name: string;
  onClose: () => void;
}) {
  return (
    <aside
      className="flex w-80 shrink-0 flex-col border-l border-border/60 bg-card p-4"
      data-testid="org-drill-panel"
    >
      <div className="mb-3 flex items-start justify-between gap-2">
        <div className="min-w-0">
          <h2 className="truncate text-base font-semibold">{name}</h2>
          <p className="text-2xs text-muted-foreground">{node.domain}</p>
        </div>
        <button
          className="text-2xs text-muted-foreground hover:text-foreground"
          onClick={onClose}
          type="button"
        >
          Close
        </button>
      </div>
      <dl className="space-y-2 text-sm">
        <div>
          <dt className="text-2xs uppercase tracking-wide text-muted-foreground">
            Reports to
          </dt>
          <dd className="truncate font-mono text-xs">
            {truncatePubkey(node.manager)}
          </dd>
        </div>
        <div>
          <dt className="text-2xs uppercase tracking-wide text-muted-foreground">
            Duties
          </dt>
          <dd className="whitespace-pre-wrap text-sm">{node.duties || "—"}</dd>
        </div>
        <div>
          <dt className="text-2xs uppercase tracking-wide text-muted-foreground">
            Cadence
          </dt>
          <dd>{node.cadence || "convention only"}</dd>
        </div>
        <div>
          <dt className="text-2xs uppercase tracking-wide text-muted-foreground">
            Budget
          </dt>
          <dd>
            {node.budget.tokensPerDay} tokens/day · {node.budget.openWorkCap}{" "}
            open self-initiated
          </dd>
        </div>
      </dl>
      <p className="mt-4 text-2xs text-muted-foreground">
        Threads, receipts, and focus live in the agent&apos;s rooms. This panel
        is a projection of the founder-signed roster.
      </p>
    </aside>
  );
}
