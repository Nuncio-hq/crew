import * as React from "react";

import type { Editor } from "@tiptap/react";

import {
  assignMentionHighlightNames,
  mentionHighlightKey,
} from "./mentionHighlightExtension";

type MentionHighlightStorage = {
  names: string[];
  agentNames: string[];
  agentAvatarsByName: Record<string, string>;
  channelNames: string[];
};

/** Crew: sync composer mention chips (incl. agent avatars) with highlight storage. */
export function useRichTextEditorMentionHighlightSync(
  editor: Editor | null,
  {
    agentAvatarUrlsByName,
    agentMentionNames,
    addressedAgentMentionNames,
    channelNames,
    mentionNames,
  }: {
    agentAvatarUrlsByName?: Record<string, string>;
    agentMentionNames?: string[];
    addressedAgentMentionNames?: readonly string[];
    channelNames?: string[];
    mentionNames?: string[];
  },
) {
  React.useEffect(() => {
    if (!editor) return;
    // biome-ignore lint/suspicious/noExplicitAny: TipTap's Storage type doesn't include dynamic extension keys
    const storage = (editor.storage as any).mentionHighlight as
      | MentionHighlightStorage
      | undefined;
    if (storage) {
      const namesChanged = assignMentionHighlightNames(
        storage,
        mentionNames ?? [],
        [
          ...new Set([
            ...(agentMentionNames ?? []),
            ...(addressedAgentMentionNames ?? []),
          ]),
        ],
        channelNames ?? [],
      );
      const avatars = agentAvatarUrlsByName ?? {};
      const avatarsChanged =
        Object.keys(avatars).length !==
          Object.keys(storage.agentAvatarsByName).length ||
        Object.entries(avatars).some(
          ([name, url]) => storage.agentAvatarsByName[name] !== url,
        );
      if (!namesChanged && !avatarsChanged) return;
      storage.agentAvatarsByName = avatars;
      const { tr } = editor.state;
      editor.view.dispatch(tr.setMeta(mentionHighlightKey, true));
    }
  }, [
    editor,
    mentionNames,
    agentMentionNames,
    addressedAgentMentionNames,
    agentAvatarUrlsByName,
    channelNames,
  ]);
}
