import * as React from "react";

import { deleteMessage } from "@/shared/api/tauri";
import { useInboxEditMessage } from "@/features/home/useInboxEditMessage";
import { DeleteMessageConfirmDialog } from "@/features/messages/ui/DeleteMessageConfirmDialog";
import type { InboxItem } from "@/features/home/lib/inbox";
import type { Channel } from "@/shared/api/types";

export function useHomeInboxEdit({
  selectedChannel,
  selectedItem,
  canDelete,
  selectedConversationId,
  refreshStructuralEvents,
  onRefresh,
}: {
  selectedChannel: Channel | null;
  selectedItem: InboxItem | null;
  canDelete: boolean;
  selectedConversationId: string | null;
  refreshStructuralEvents: () => Promise<void>;
  onRefresh: () => void;
}) {
  const { editMessage, isEditingMessage } = useInboxEditMessage(
    selectedChannel,
    refreshStructuralEvents,
  );
  const [isDeletingMessage, setIsDeletingMessage] = React.useState(false);
  const [emptyDeleteId, setEmptyDeleteId] = React.useState<string | null>(null);
  const [editTargetId, setEditTargetId] = React.useState<string | null>(null);

  React.useEffect(() => {
    if (selectedConversationId) {
      setEmptyDeleteId(null);
      setEditTargetId(null);
    }
  }, [selectedConversationId]);

  const channelId = selectedChannel?.id ?? selectedItem?.item.channelId;

  const deleteInboxMessage = React.useCallback(
    async (eventId: string) => {
      if (!channelId) return;
      setIsDeletingMessage(true);
      try {
        await deleteMessage(channelId, eventId);
        await refreshStructuralEvents();
        onRefresh();
      } finally {
        setIsDeletingMessage(false);
      }
    },
    [channelId, refreshStructuralEvents, onRefresh],
  );

  const onDelete = React.useCallback(() => {
    if (!selectedItem || !canDelete) return;
    void deleteInboxMessage(selectedItem.id);
  }, [selectedItem, canDelete, deleteInboxMessage]);

  const dialog = (
    <DeleteMessageConfirmDialog
      onConfirm={() => {
        if (emptyDeleteId) {
          setEditTargetId(null);
          void deleteInboxMessage(emptyDeleteId);
        }
        setEmptyDeleteId(null);
      }}
      onOpenChange={(open) => {
        if (!open) setEmptyDeleteId(null);
      }}
      open={emptyDeleteId !== null}
    />
  );

  return {
    dialog,
    isDeletingMessage,
    isEditingMessage,
    editMessage,
    editTargetId,
    setEditTargetId,
    setEmptyDeleteId,
    onRequestEmptyEditDelete: setEmptyDeleteId,
    onDelete,
  };
}
