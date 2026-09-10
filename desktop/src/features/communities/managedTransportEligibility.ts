import { invokeTauri } from "@/shared/api/tauri";

type UpdateEligibility = (relayUrl: string, enabled: boolean) => Promise<void>;

/** Serialize only side effects; native generations remain the authority. */
export function createTransportEligibilityQueue(
  update: UpdateEligibility,
): UpdateEligibility {
  let tail = Promise.resolve();
  let pending = 0;
  return (relayUrl, enabled) => {
    if (pending >= 32)
      return Promise.reject(
        new Error("Community updates are busy. Try again."),
      );
    pending += 1;
    const result = tail.then(() => update(relayUrl, enabled));
    tail = result.then(
      () => {
        pending -= 1;
      },
      () => {
        pending -= 1;
      },
    );
    return result;
  };
}

export const setManagedTransportEligibility = createTransportEligibilityQueue(
  (relayUrl, enabled) =>
    invokeTauri<void>("set_managed_transport_eligibility", {
      relayUrl,
      enabled,
    }),
);

/** Local removal happens only after leave and native invalidation succeed. */
export async function completeCommunityRemoval<T>(
  leave: () => Promise<T>,
  invalidate: () => Promise<void>,
  removeLocal: () => void | Promise<void>,
): Promise<T> {
  const result = await leave();
  await invalidate();
  await removeLocal();
  return result;
}
