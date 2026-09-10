import type { UserInputEvent } from "../lib/userInput";

type AnswerClaim = { request: UserInputEvent };
const MAX_UNSETTLED_ANSWERS = 128;

/** Ephemeral publication ownership over already-authorized durable requests. */
export function createUserInputAnswerGate(scope: object) {
  let retired = false;
  let pending = new Map<string, UserInputEvent>();
  const inFlight = new Map<string, AnswerClaim>();
  const accepted = new Set<string>();
  return {
    scope,
    retire() {
      retired = true;
      pending.clear();
      inFlight.clear();
      accepted.clear();
    },
    reconcile(requests: readonly UserInputEvent[]) {
      if (retired) return;
      pending = new Map(requests.map((request) => [request.event.id, request]));
      for (const id of accepted) if (!pending.has(id)) accepted.delete(id);
    },
    claim(id: string): AnswerClaim | { error: string } | null {
      const request = pending.get(id);
      if (retired || !request || accepted.has(id) || inFlight.has(id))
        return null;
      if (inFlight.size + accepted.size >= MAX_UNSETTLED_ANSWERS) {
        return {
          error:
            "Too many answers are awaiting confirmation. Retry after they synchronize.",
        };
      }
      const claim = { request };
      inFlight.set(id, claim);
      return claim;
    },
    isCurrent(claim: AnswerClaim) {
      return !retired && inFlight.get(claim.request.event.id) === claim;
    },
    accept(claim: AnswerClaim) {
      if (!retired && inFlight.get(claim.request.event.id) === claim) {
        accepted.add(claim.request.event.id);
      }
    },
    release(claim: AnswerClaim) {
      if (retired || inFlight.get(claim.request.event.id) !== claim)
        return false;
      inFlight.delete(claim.request.event.id);
      return true;
    },
  };
}
