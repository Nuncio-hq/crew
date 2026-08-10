import { Badge } from "@/shared/ui/badge";
import type { HermesProfileReadiness } from "@/shared/api/types";
import { presentHermesProfileReadiness } from "../hermesProfileReadinessPresenter";

export function HermesProfileReadinessIndicator({
  readiness,
}: {
  readiness: HermesProfileReadiness;
}) {
  const presentation = presentHermesProfileReadiness(readiness);
  return (
    <Badge
      data-testid={presentation.testId}
      title={`${presentation.explanation} ${presentation.repair}`}
      variant={presentation.tone === "blocking" ? "warning" : "secondary"}
    >
      {presentation.label}
    </Badge>
  );
}
