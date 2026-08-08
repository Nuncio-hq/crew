import type { VListHandle } from "virtua";

/**
 * Keep a bounded ID-keyed neighborhood around the reader plus the visual tail.
 * The eviction band is wider than the admission band to prevent small direction
 * changes from churning mounted rows. Virtua owns measured sizes and spacer
 * math, so retaining more DOM than this only increases scroll/reflow cost.
 */
export function nextRetainedTimelineKeys(
  keys: readonly string[],
  previous: ReadonlySet<string>,
  list: VListHandle,
): ReadonlySet<string> {
  const viewportSize = Math.max(list.viewportSize, 1);
  const offset = list.scrollOffset;
  const indexAt = (target: number) =>
    list.findItemIndex(Math.min(list.scrollSize, Math.max(0, target)));
  const admissionStart = indexAt(offset - viewportSize * 3);
  const admissionEnd = indexAt(offset + viewportSize * 4);
  const evictionStart = indexAt(offset - viewportSize * 5);
  const evictionEnd = indexAt(offset + viewportSize * 6);
  const tailStart = indexAt(list.scrollSize - viewportSize * 2);
  const next = new Set<string>();

  for (let index = evictionStart; index <= evictionEnd; index += 1) {
    const key = keys[index];
    if (key && previous.has(key)) next.add(key);
  }
  for (let index = admissionStart; index <= admissionEnd; index += 1) {
    const key = keys[index];
    if (key) next.add(key);
  }
  for (let index = tailStart; index < keys.length; index += 1) {
    const key = keys[index];
    if (key) next.add(key);
  }

  return next.size === previous.size &&
    [...next].every((key) => previous.has(key))
    ? previous
    : next;
}
