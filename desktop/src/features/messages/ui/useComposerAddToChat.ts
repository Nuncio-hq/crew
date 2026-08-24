import * as React from "react";

import type { UseRichTextEditorResult } from "@/features/messages/lib/useRichTextEditor";
import { useAddToChat } from "./AddToChatContext";

export function useComposerAddToChat(
  disabled: boolean,
  richText: UseRichTextEditorResult,
  scrollToBottom: () => void,
) {
  const addToChat = useAddToChat();
  const insertSelectedText = React.useCallback(
    (text: string) => {
      const editor = richText.editor;
      if (!editor || disabled) return false;
      const inserted = editor
        .chain()
        .focus("end")
        .insertContent({
          type: "blockquote",
          content: text.split("\n").map((line) => ({
            type: "paragraph",
            content: line ? [{ type: "text", text: line }] : undefined,
          })),
        })
        .insertContent({ type: "paragraph" })
        .run();
      scrollToBottom();
      return inserted;
    },
    [disabled, richText.editor, scrollToBottom],
  );
  React.useEffect(
    () => addToChat?.registerComposer(insertSelectedText),
    [addToChat, insertSelectedText],
  );
}
