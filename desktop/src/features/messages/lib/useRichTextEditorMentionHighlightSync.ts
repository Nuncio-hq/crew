import * as React from "react";

import type { Editor } from "@tiptap/react";

import { mentionHighlightKey } from "./mentionHighlightExtension";

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
    channelNames,
    mentionNames,
  }: {
    agentAvatarUrlsByName?: Record<string, string>;
    agentMentionNames?: string[];
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
      storage.names = mentionNames ?? [];
      storage.agentNames = agentMentionNames ?? [];
      storage.agentAvatarsByName = agentAvatarUrlsByName ?? {};
      storage.channelNames = channelNames ?? [];
      const { tr } = editor.state;
      editor.view.dispatch(tr.setMeta(mentionHighlightKey, true));
    }
  }, [
    editor,
    mentionNames,
    agentMentionNames,
    agentAvatarUrlsByName,
    channelNames,
  ]);
}
