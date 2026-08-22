import type { AuxiliaryPanelClose } from "@/shared/layout/auxiliaryPanelContext";

type ThreadPanelCloseOptions = {
  isFocusDrawer: boolean;
  onDismissThread: () => void;
  onMinimizeThread: () => void;
  useSplitAuxiliaryPane: boolean;
};

const MINIMIZE_LABEL = "Minimize thread";

/**
 * Routes the thread panel's close control between minimize and dismiss.
 *
 * In the focus drawer the thread is a presentation of channel state, not a
 * separate destination: the drawer already has the scrim and Escape as ways
 * out, so its X is the one control that can hand the thread back to the split
 * pane instead of throwing the reading context away. That only holds where a
 * split pane exists to minimize into — narrow and overlay presentations have
 * nowhere to put the thread, so their X stays a full dismiss.
 */
export function resolveThreadPanelClose({
  isFocusDrawer,
  onDismissThread,
  onMinimizeThread,
  useSplitAuxiliaryPane,
}: ThreadPanelCloseOptions): AuxiliaryPanelClose {
  if (isFocusDrawer && useSplitAuxiliaryPane) {
    return { closeLabel: MINIMIZE_LABEL, onClose: onMinimizeThread };
  }

  return { onClose: onDismissThread };
}
