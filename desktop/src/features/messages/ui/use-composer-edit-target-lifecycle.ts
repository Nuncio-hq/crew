import * as React from "react";

import type { QueuedMediaAttachment } from "@/features/messages/lib/backgroundMediaUploadStore";
import {
  findSpoileredImetaMediaUrls,
  type ImetaMedia,
  restoreImetaMediaDisplayLabels,
  stripImetaMediaLines,
} from "@/features/messages/lib/imetaMediaMarkdown";
import type { UseRichTextEditorResult } from "@/features/messages/lib/useRichTextEditor";

type EditTarget = { body: string; id: string; imetaMedia?: ImetaMedia[] };

export function useComposerEditTargetLifecycle({
  editTarget,
  media,
  onCancelEdit,
  preEditSnapshotRef,
  richText,
  setComposerContent,
  setSpoileredAttachmentUrls,
  spoileredAttachmentUrls,
  syncComposerContentFromEditor,
}: {
  editTarget: EditTarget | null;
  media: {
    clearQueuedAttachments: () => void;
    isUploading: boolean;
    pendingImetaRef: React.MutableRefObject<ImetaMedia[]>;
    queuedAttachmentsRef: React.MutableRefObject<QueuedMediaAttachment[]>;
    restoreQueuedAttachments: (attachments: QueuedMediaAttachment[]) => void;
    setPendingImeta: (pendingImeta: ImetaMedia[]) => void;
  };
  onCancelEdit?: () => void;
  preEditSnapshotRef: React.MutableRefObject<{
    content: string;
    pendingImeta: ImetaMedia[];
    queuedAttachments: QueuedMediaAttachment[];
    spoileredAttachmentUrls: Set<string>;
  } | null>;
  richText: Pick<
    UseRichTextEditorResult,
    "clearContent" | "focusEnd" | "setContent"
  >;
  setComposerContent: (content: string) => void;
  setSpoileredAttachmentUrls: React.Dispatch<React.SetStateAction<Set<string>>>;
  spoileredAttachmentUrls: Set<string>;
  syncComposerContentFromEditor: () => string;
}) {
  // biome-ignore lint/correctness/useExhaustiveDependencies: editTarget?.id is the trigger
  React.useEffect(() => {
    if (editTarget && media.isUploading) {
      onCancelEdit?.();
      return;
    }
    if (editTarget) {
      preEditSnapshotRef.current = {
        content: syncComposerContentFromEditor(),
        pendingImeta: [...media.pendingImetaRef.current],
        queuedAttachments: [...media.queuedAttachmentsRef.current],
        spoileredAttachmentUrls: new Set(spoileredAttachmentUrls),
      };
      const editableImeta = restoreImetaMediaDisplayLabels(
        editTarget.body,
        editTarget.imetaMedia ?? [],
      );
      const editableBody = stripImetaMediaLines(editTarget.body, editableImeta);
      setComposerContent(editableBody);
      richText.setContent(editableBody);
      media.setPendingImeta(editableImeta);
      media.clearQueuedAttachments();
      setSpoileredAttachmentUrls(
        findSpoileredImetaMediaUrls(editTarget.body, editableImeta),
      );
      const rafId = requestAnimationFrame(() => richText.focusEnd());
      return () => cancelAnimationFrame(rafId);
    }
    if (preEditSnapshotRef.current !== null) {
      const snapshot = preEditSnapshotRef.current;
      preEditSnapshotRef.current = null;
      setComposerContent(snapshot.content);
      snapshot.content
        ? richText.setContent(snapshot.content)
        : richText.clearContent();
      media.setPendingImeta(snapshot.pendingImeta);
      media.restoreQueuedAttachments(snapshot.queuedAttachments);
      setSpoileredAttachmentUrls(snapshot.spoileredAttachmentUrls);
    }
  }, [editTarget?.id]);
}
