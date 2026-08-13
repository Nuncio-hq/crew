export type WorkbenchLens = "thread" | "agent";

export type WorkbenchSearch = {
  lens?: WorkbenchLens;
  office?: boolean;
  messageId?: string;
};

export function parseWorkbenchLens(value: unknown): WorkbenchLens {
  return value === "agent" ? "agent" : "thread";
}

export function workbenchHref(
  channelId?: string | null,
  threadRootId?: string | null,
  search: WorkbenchSearch = {},
): string {
  const params = new URLSearchParams();
  if (search.lens === "agent") params.set("lens", "agent");
  if (search.office) params.set("office", "1");
  if (search.messageId) params.set("messageId", search.messageId);
  const query = params.toString();
  const suffix = query ? `?${query}` : "";
  if (!channelId || !threadRootId) {
    return `/workbench${suffix}`;
  }
  return `/workbench/${encodeURIComponent(channelId)}/${encodeURIComponent(threadRootId)}${suffix}`;
}

export function channelHrefFromWorkbench(
  channelId: string,
  threadRootId: string,
  messageId?: string | null,
): string {
  const params = new URLSearchParams();
  params.set("thread", threadRootId);
  if (messageId) {
    params.set("messageId", messageId);
    params.set("threadRootId", threadRootId);
  }
  return `/channels/${encodeURIComponent(channelId)}?${params.toString()}`;
}
