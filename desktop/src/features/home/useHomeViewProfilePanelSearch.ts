import * as React from "react";

import type {
  ProfilePanelTab,
  ProfilePanelView,
} from "@/features/profile/ui/UserProfilePanel";

type ApplyInboxSearchPatch = (
  patch: {
    profile?: string | null;
    profileTab?: ProfilePanelTab | null;
    profileView?: ProfilePanelView | null;
  },
  options?: { replace?: boolean },
) => void;

export function useHomeViewProfilePanelSearch(
  applyInboxSearchPatch: ApplyInboxSearchPatch,
  clearVerifiedTarget: () => void,
  setManagedChannelId: (channelId: string | null) => void,
) {
  const handleOpenProfilePanel = React.useCallback(
    (pubkey: string) => {
      clearVerifiedTarget();
      setManagedChannelId(null);
      applyInboxSearchPatch({
        profile: pubkey,
        profileTab: null,
        profileView: null,
      });
    },
    [applyInboxSearchPatch, clearVerifiedTarget, setManagedChannelId],
  );

  const handleCloseProfilePanel = React.useCallback(() => {
    clearVerifiedTarget();
    applyInboxSearchPatch({
      profile: null,
      profileTab: null,
      profileView: null,
    });
  }, [applyInboxSearchPatch, clearVerifiedTarget]);

  const handleProfilePanelViewChange = React.useCallback(
    (view: ProfilePanelView, options?: { replace?: boolean }) =>
      applyInboxSearchPatch(
        { profileView: view === "summary" ? null : view },
        options,
      ),
    [applyInboxSearchPatch],
  );

  const handleProfilePanelTabChange = React.useCallback(
    (tab: ProfilePanelTab, options?: { replace?: boolean }) =>
      applyInboxSearchPatch(
        { profileTab: tab === "info" ? null : tab },
        options,
      ),
    [applyInboxSearchPatch],
  );

  return {
    handleCloseProfilePanel,
    handleOpenProfilePanel,
    handleProfilePanelTabChange,
    handleProfilePanelViewChange,
  };
}
