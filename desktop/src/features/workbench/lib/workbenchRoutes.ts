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
  if (!channelId || !threadRootId) {
    return "/";
  }
  return channelHrefFromWorkbench(channelId, threadRootId, search.messageId);
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
