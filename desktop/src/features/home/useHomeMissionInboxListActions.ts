import * as React from "react";

import type { MissionInboxRow } from "@/features/home/lib/missionInbox";
import type { MissionInboxEventTarget } from "@/features/home/lib/missionInbox";

export function useHomeMissionInboxListActions({
  clearVerifiedTarget,
  handleMissionSelect,
  markItemRead,
  setSelectedDraftKey,
  setSelectedReminderId,
  setUnreadBoundary,
  setUnreadOnly,
}: {
  clearVerifiedTarget: () => void;
  handleMissionSelect: (
    row: MissionInboxRow,
  ) => Promise<MissionInboxEventTarget | null>;
  markItemRead: (itemId: string) => void;
  setSelectedDraftKey: (key: string | null) => void;
  setSelectedReminderId: (id: string | null) => void;
  setUnreadBoundary: React.Dispatch<
    React.SetStateAction<{
      conversationId: string;
      eventId: string;
    } | null>
  >;
  setUnreadOnly: React.Dispatch<React.SetStateAction<boolean>>;
}) {
  const handleSelectMission = React.useCallback(
    (row: MissionInboxRow) => {
      setUnreadBoundary(null);
      setSelectedDraftKey(null);
      setSelectedReminderId(null);
      void handleMissionSelect(row).then((target) => {
        if (target) markItemRead(target.messageId);
      });
    },
    [
      handleMissionSelect,
      markItemRead,
      setSelectedDraftKey,
      setSelectedReminderId,
      setUnreadBoundary,
    ],
  );

  const handleUnreadOnlyChange = React.useCallback(
    (nextUnreadOnly: boolean) => {
      clearVerifiedTarget();
      setUnreadOnly(nextUnreadOnly);
    },
    [clearVerifiedTarget, setUnreadOnly],
  );

  return {
    handleSelectMission,
    handleUnreadOnlyChange,
  };
}
