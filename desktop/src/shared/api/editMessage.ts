import { invokeTauri } from "@/shared/api/tauri";

export type EditMessageInput = {
  channelId: string;
  eventId: string;
  content: string;
  mediaTags: string[][];
  emojiTags: string[][];
  mentionPubkeys: string[];
  suppressLinkPreviews: boolean;
  removedMentionPubkeys: string[];
};

export async function editMessage(
  channelId: string,
  eventId: string,
  content: string,
  mediaTags?: string[][],
  emojiTags?: string[][],
  mentionPubkeys?: string[],
  suppressLinkPreviews?: boolean,
  removedMentionPubkeys?: string[],
): Promise<void> {
  await invokeTauri("edit_message", {
    input: {
      channelId,
      eventId,
      content,
      mediaTags: mediaTags ?? [],
      emojiTags: emojiTags ?? [],
      mentionPubkeys: mentionPubkeys ?? [],
      suppressLinkPreviews: suppressLinkPreviews ?? false,
      removedMentionPubkeys: removedMentionPubkeys ?? [],
    },
  });
}
