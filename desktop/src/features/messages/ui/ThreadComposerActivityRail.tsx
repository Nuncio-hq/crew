import type { Channel } from "@/shared/api/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { THREAD_PANEL_COMPOSER_GUTTER_CLASS } from "@/features/messages/lib/messageThreadPanelLayout";
import { ComposerActivityAccessory } from "./ComposerActivityAccessory";
import { TypingIndicatorRow } from "./TypingIndicatorRow";

/**
 * Thread dock's bottom activity rail. Extracted from MessageThreadPanel so the
 * panel stays inside the desktop file-size ratchet (D-022).
 *
 * The accessory is anchored in the dock's reserved bottom rail, so fading it
 * cannot change the observed overlay height or move the conversation. Its
 * natural content height remains responsive.
 */
export function ThreadComposerActivityRail({
  activityAccessoryContent,
  channel,
  currentPubkey,
  profiles,
  typingPubkeys,
  visible,
}: {
  activityAccessoryContent?: React.ReactNode;
  channel: Channel | null;
  currentPubkey?: string;
  profiles?: UserProfileLookup;
  typingPubkeys: string[];
  visible: boolean;
}) {
  return (
    <ComposerActivityAccessory
      className={THREAD_PANEL_COMPOSER_GUTTER_CLASS}
      visible={visible}
    >
      <div className="mx-auto flex w-full max-w-4xl items-center gap-2 overflow-visible pl-2">
        {activityAccessoryContent ? (
          <div className="flex min-w-0 flex-1 overflow-visible">
            {activityAccessoryContent}
          </div>
        ) : null}
        {typingPubkeys.length > 0 ? (
          <TypingIndicatorRow
            channel={channel}
            className="min-w-0 flex-1 py-0 pl-[calc(0.75rem+1px)] pr-0 [@container(min-width:40rem)]:pl-[calc(1rem+1px)]"
            currentPubkey={currentPubkey}
            profiles={profiles}
            typingPubkeys={typingPubkeys}
            variant="activity"
          />
        ) : null}
      </div>
    </ComposerActivityAccessory>
  );
}
