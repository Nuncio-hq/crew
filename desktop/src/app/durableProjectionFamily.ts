import type { RelayEvent } from "@/shared/api/types";

export type DurableProjectionFamily = "receipt" | "userInput";
export type DurableProjectionFamilyState<T> = Record<
  DurableProjectionFamily,
  T
>;

export function createDurableProjectionFamilyCounters(): DurableProjectionFamilyState<number> {
  return { receipt: 0, userInput: 0 };
}

export function activeFamilyStateIsCurrent(
  current: DurableProjectionFamilyState<number>,
  snapshot: DurableProjectionFamilyState<number>,
  active: DurableProjectionFamilyState<boolean>,
): boolean {
  return (
    (!active.userInput || current.userInput === snapshot.userInput) &&
    (!active.receipt || current.receipt === snapshot.receipt)
  );
}

export function eventBelongsToActiveProjection(
  event: RelayEvent,
  familyForEvent: (candidate: RelayEvent) => DurableProjectionFamily | null,
  active: DurableProjectionFamilyState<boolean>,
): boolean {
  const family = familyForEvent(event);
  return family !== null && active[family];
}
