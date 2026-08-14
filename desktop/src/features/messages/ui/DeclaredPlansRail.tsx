import { useSharedNowWhen } from "@/features/agents/lib/sharedNow";
import type { AgentDeclaredPlan } from "@/features/agents/declaredPlanProjection";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { DECLARED_PLANS_RAIL_WIDTH_PX } from "@/shared/layout/responsiveContract";
import { cn } from "@/shared/lib/cn";

import { DeclaredPlanCard } from "./DeclaredPlanCard";

/**
 * Declared-plans rail (#190 / #205 reference case).
 *
 * Min: stacks under the thread header whenever the pane cannot fit
 * `w-72` plus a readable column. Never overlaps chrome. Truncate titles;
 * never squeeze.
 */
export function DeclaredPlansRail({
  className,
  layout = "side",
  plans,
  profiles,
}: {
  className?: string;
  layout?: "side" | "stacked";
  plans: readonly AgentDeclaredPlan[];
  profiles?: UserProfileLookup;
}) {
  const now = useSharedNowWhen(plans.length > 0);
  if (plans.length === 0) return null;

  return (
    <aside
      aria-label="Declared plans"
      className={cn(
        "flex min-w-0 shrink-0 flex-col overflow-y-auto overflow-x-hidden bg-background/40 px-2.5 py-3",
        layout === "side"
          ? "border-l border-border/60"
          : "max-h-48 border-b border-border/60",
        className,
      )}
      data-layout={layout}
      data-testid="declared-plans-rail"
      style={
        layout === "side"
          ? { width: DECLARED_PLANS_RAIL_WIDTH_PX }
          : { width: "100%" }
      }
    >
      <h2 className="mb-2 truncate px-0.5 text-2xs font-semibold tracking-wide text-muted-foreground uppercase">
        Declared plans
      </h2>
      <div className="flex min-w-0 flex-col gap-2">
        {plans.map((plan) => (
          <DeclaredPlanCard
            key={plan.agentPubkey}
            now={now}
            plan={plan}
            profiles={profiles}
          />
        ))}
      </div>
    </aside>
  );
}
