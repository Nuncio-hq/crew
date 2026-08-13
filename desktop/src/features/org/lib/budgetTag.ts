import {
  CREW_BUDGET_STOP,
  CREW_BUDGET_TAG,
} from "@/features/org/lib/orgRoster";

export function isBudgetStopMessage(
  tags: readonly (readonly string[])[] | undefined,
): boolean {
  return Boolean(
    tags?.some(
      (tag) => tag[0] === CREW_BUDGET_TAG && tag[1] === CREW_BUDGET_STOP,
    ),
  );
}
