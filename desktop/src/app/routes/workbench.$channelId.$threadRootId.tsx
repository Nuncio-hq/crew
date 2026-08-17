import { createFileRoute, redirect } from "@tanstack/react-router";

export const Route = createFileRoute("/workbench/$channelId/$threadRootId")({
  beforeLoad: ({ params, search }) => {
    const messageId =
      search &&
      typeof search === "object" &&
      "messageId" in search &&
      typeof search.messageId === "string"
        ? search.messageId
        : undefined;
    throw redirect({
      to: "/channels/$channelId",
      params: { channelId: params.channelId },
      search: {
        thread: params.threadRootId,
        ...(messageId ? { messageId, threadRootId: params.threadRootId } : {}),
      },
    });
  },
});
