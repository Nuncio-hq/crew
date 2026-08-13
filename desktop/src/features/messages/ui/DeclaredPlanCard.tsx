import { cn } from "@/shared/lib/cn";
import type {
  AgentDeclaredPlan,
  DeclaredPlanEntry,
} from "@/features/agents/declaredPlanProjection";
import { formatCompactAgo } from "@/features/agents/ui/agentSessionUtils";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { normalizePubkey } from "@/shared/lib/pubkey";
import { UserAvatar } from "@/shared/ui/UserAvatar";

export function DeclaredPlanCard({
  now,
  plan,
  profiles,
}: {
  now: number;
  plan: AgentDeclaredPlan;
  profiles?: UserProfileLookup;
}) {
  const profile =
    profiles?.[plan.agentPubkey] ??
    profiles?.[normalizePubkey(plan.agentPubkey)];
  const statusLine = declaredPlanStatusLine(plan, now);

  return (
    <section
      className={cn(
        "rounded-lg border px-2.5 py-2",
        plan.unknown
          ? "border-dashed border-muted-foreground/35 bg-muted/20 text-muted-foreground"
          : "border-border/70 bg-card/60",
      )}
      data-testid={`declared-plan-card-${plan.agentPubkey}`}
    >
      <header className="flex min-w-0 items-start gap-2">
        <UserAvatar
          avatarUrl={profile?.avatarUrl ?? null}
          className="mt-0.5"
          displayName={plan.agentName}
          size="xs"
        />
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium text-foreground">
            {plan.agentName}
          </p>
          <p className="text-2xs text-muted-foreground">{statusLine}</p>
        </div>
      </header>
      {plan.unknown ? (
        <p className="mt-2 text-xs text-muted-foreground/80">
          No ACP plan or structured todo snapshot.
        </p>
      ) : (
        <ul className="mt-2 space-y-1">
          {plan.entries.map((entry, index) => (
            <DeclaredPlanRow
              // biome-ignore lint/suspicious/noArrayIndexKey: latest snapshot rows have no stable ids
              key={index}
              entry={entry}
            />
          ))}
        </ul>
      )}
    </section>
  );
}

export function declaredPlanStatusLine(
  plan: AgentDeclaredPlan,
  now: number,
): string {
  const ago = plan.updatedAt
    ? formatCompactAgo(Math.max(0, now - Date.parse(plan.updatedAt)))
    : null;
  switch (plan.liveness) {
    case "working":
      return ago ? `working · updated ${ago}` : "working";
    case "sleeping":
      return ago ? `sleeping · last declared ${ago}` : "sleeping";
    case "disconnected":
      if (plan.unknown) return "disconnected · plan unknown";
      return ago ? `disconnected · last declared ${ago}` : "disconnected";
    case "idle":
      if (plan.unknown) return "plan unknown";
      return ago ? `last declared ${ago}` : "idle";
    default: {
      const exhaustive: never = plan.liveness;
      return exhaustive;
    }
  }
}

function DeclaredPlanRow({ entry }: { entry: DeclaredPlanEntry }) {
  const checked = entry.status === "completed";
  return (
    <li
      className={cn(
        "flex min-w-0 items-start gap-2 text-sm leading-5 text-muted-foreground",
        entry.status === "in_progress" && "text-foreground",
      )}
    >
      <input
        checked={checked}
        className="mt-0.5 h-3.5 w-3.5 shrink-0 cursor-default accent-primary"
        disabled
        readOnly
        type="checkbox"
      />
      <span className="min-w-0 wrap-break-word">
        {entry.content}
        {entry.status === "in_progress" ? (
          <span className="ml-1 text-2xs text-muted-foreground">
            ← in_progress
          </span>
        ) : null}
      </span>
    </li>
  );
}
