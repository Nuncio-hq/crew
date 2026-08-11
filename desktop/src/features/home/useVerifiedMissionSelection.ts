import * as React from "react";

import {
  getMissionInboxEventTarget,
  type MissionInboxEventTarget,
  type MissionInboxRow,
} from "@/features/home/lib/missionInbox";

type ApplyInboxItemSelection = (patch: { item: string | null }) => void;
type OpenMissionContext = (
  channelId: string,
  messageId: string,
  threadRootId: string,
) => void;

export function useVerifiedMissionSelection(
  selectedEventId: string | null,
  setAutoSelectedEventId: (eventId: string | null) => void,
  applyInboxSearchPatch: ApplyInboxItemSelection,
  openMissionContext: OpenMissionContext,
) {
  const [verifiedTarget, setVerifiedTarget] =
    React.useState<MissionInboxEventTarget | null>(null);
  const selectionGenerationRef = React.useRef(0);

  const clearVerifiedTarget = React.useCallback(() => {
    selectionGenerationRef.current += 1;
    setVerifiedTarget(null);
  }, []);

  const openMissionRow = React.useCallback(
    async (row: MissionInboxRow) => {
      const generation = ++selectionGenerationRef.current;
      const target = await getMissionInboxEventTarget(row);
      if (!target || selectionGenerationRef.current !== generation) return null;
      openMissionContext(
        target.channelId,
        target.messageId,
        target.threadRootId,
      );
      return target;
    },
    [openMissionContext],
  );

  const selectMissionRow = React.useCallback(
    async (row: MissionInboxRow) => {
      if (!row.inboxItem) return openMissionRow(row);
      const generation = ++selectionGenerationRef.current;
      const target = await getMissionInboxEventTarget(row);
      if (!target || selectionGenerationRef.current !== generation) return null;
      setVerifiedTarget(target);
      setAutoSelectedEventId(null);
      applyInboxSearchPatch({ item: target.messageId });
      return target;
    },
    [applyInboxSearchPatch, openMissionRow, setAutoSelectedEventId],
  );

  React.useEffect(() => {
    selectionGenerationRef.current += 1;
    if (verifiedTarget && verifiedTarget.messageId !== selectedEventId) {
      setVerifiedTarget(null);
    }
  }, [selectedEventId, verifiedTarget]);

  React.useEffect(
    () => () => {
      selectionGenerationRef.current += 1;
    },
    [],
  );

  return {
    activeVerifiedTarget:
      verifiedTarget?.messageId === selectedEventId ? verifiedTarget : null,
    clearVerifiedTarget,
    openMissionRow,
    selectMissionRow,
  };
}
