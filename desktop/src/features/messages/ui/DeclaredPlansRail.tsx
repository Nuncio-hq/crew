import { useSharedNowWhen } from "@/features/agents/lib/sharedNow";
import type { AgentDeclaredPlan } from "@/features/agents/declaredPlanProjection";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { cn } from "@/shared/lib/cn";

import { DeclaredPlanCard } from "./DeclaredPlanCard";

export function DeclaredPlansRail({
  className,
  plans,
  profiles,
}: {
  className?: string;
  plans: readonly AgentDeclaredPlan[];
  profiles?: UserProfileLookup;
}) {
  const now = useSharedNowWhen(plans.length > 0);
  if (plans.length === 0) return null;

  return (
    <aside
      aria-label="Declared plans"
      className={cn(
        "flex w-72 shrink-0 flex-col overflow-y-auto overflow-x-hidden border-l border-border/60 bg-background/40 px-2.5 py-3",
        className,
      )}
      data-testid="declared-plans-rail"
    >
      <h2 className="mb-2 px-0.5 text-2xs font-semibold tracking-wide text-muted-foreground uppercase">
        Declared plans
      </h2>
      <div className="flex flex-col gap-2">
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
