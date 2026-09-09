import type { AuxiliaryPanelClose } from "@/shared/layout/auxiliaryPanelContext";
import * as React from "react";
import { ThreadFocusForgeSplit } from "@/features/messages/ui/threadPrHub/ThreadFocusForgeSplit";

import { FocusThreadDrawer } from "@/features/channels/ui/FocusThreadDrawer";
import { usePresenceCoverage } from "@/features/channels/ui/useFocusDrawerPresence";
import { useBindThreadToolPaneView } from "@/features/tool-pane/useThreadToolPaneScope";

type ThreadPanelSurfaceProps = {
  channelName: string;
  channelId?: string | null;
  threadRootId?: string | null;
  panelClose?: AuxiliaryPanelClose;
  children: React.ReactNode;
  covered: boolean;
  hasActiveEdit: boolean;
  isFocusDrawer: boolean;
  /** Standalone thread views need the same tool host as the focus drawer. */
  isStandalone?: boolean;
  onClose: () => void;
};

/** Keeps a thread mounted while controlling its focus-drawer presentation. */
export const ThreadPanelSurface = React.forwardRef<
  HTMLDivElement,
  ThreadPanelSurfaceProps
>(function ThreadPanelSurface(
  {
    channelName,
    channelId,
    threadRootId,
    panelClose,
    children,
    covered,
    hasActiveEdit,
    isFocusDrawer,
    isStandalone = false,
    onClose,
  },
  ref,
) {
  useBindThreadToolPaneView(channelId, threadRootId);
  return (
    <div
      aria-hidden={covered ? true : undefined}
      className="contents"
      data-testid="thread-surface"
      inert={covered ? true : undefined}
      ref={ref}
    >
      {isFocusDrawer ? (
        <FocusThreadDrawer
          channelName={channelName}
          channelId={channelId}
          threadRootId={threadRootId}
          panelClose={panelClose}
          escapeEnabled={!covered}
          hasActiveEdit={hasActiveEdit}
          onClose={onClose}
        >
          {children}
        </FocusThreadDrawer>
      ) : isStandalone ? (
        <ThreadFocusForgeSplit
          channelId={channelId ?? null}
          channelName={channelName}
          threadRootId={threadRootId ?? null}
        >
          {children}
        </ThreadFocusForgeSplit>
      ) : (
        children
      )}
    </div>
  );
});

/** Supplies covered-thread lifecycle and focus ownership for an overlay drawer. */
export function useThreadPanelSurface(
  open: boolean,
  onExitComplete: () => void,
) {
  const ref = React.useRef<HTMLDivElement>(null);
  const coverage = usePresenceCoverage(open);
  const markExitComplete = React.useCallback(() => {
    coverage.markExitComplete();
    onExitComplete();
  }, [coverage.markExitComplete, onExitComplete]);
  const restoreFocusTarget = React.useCallback(
    () =>
      ref.current?.querySelector<HTMLElement>(
        '[data-testid="auxiliary-panel-close"]',
      ) ?? null,
    [],
  );
  return { ...coverage, markExitComplete, ref, restoreFocusTarget };
}
