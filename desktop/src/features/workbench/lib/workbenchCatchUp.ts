/**
 * Catch-up marker: first message newer than the existing thread read
 * frontier. The workbench inherits the office's async nature — owners leave
 * for hours; CLI sessions never needed this.
 */
export function firstUnreadAfterReadAt(
  messages: ReadonlyArray<{ createdAt: number; id: string }>,
  readAt: number | null,
): string | null {
  if (readAt === null) return null;
  const sorted = [...messages].sort((left, right) =>
    left.createdAt !== right.createdAt
      ? left.createdAt - right.createdAt
      : left.id.localeCompare(right.id),
  );
  for (const message of sorted) {
    if (message.createdAt > readAt) return message.id;
  }
  return null;
}
