import * as React from "react";

export type AuxiliaryPanelMode = "docked" | "panel" | "single-panel";
export type AuxiliaryPanelLayout = "standalone" | "split";

export type AuxiliaryPanelClose = {
  /** Overrides the header close action's label when it does not dismiss. */
  closeLabel?: string;
  onClose: () => void;
};

export type AuxiliaryPanelContextValue = {
  /** Overrides the header close action's label when it does not dismiss. */
  closeLabel?: string;
  isFloatingOverlay: boolean;
  isOverlay: boolean;
  isSinglePanelView: boolean;
  isSplitLayout: boolean;
  layout: AuxiliaryPanelLayout;
  mode: AuxiliaryPanelMode;
  onClose: () => void;
  transparentChrome: boolean;
  widthPx: number;
};

export const AuxiliaryPanelContext =
  React.createContext<AuxiliaryPanelContextValue | null>(null);

/**
 * Retargets the header close control of every `AuxiliaryPanel` below it.
 *
 * For surfaces that own how a panel is presented rather than whether it is
 * open — the thread focus drawer minimizes into the split pane instead of
 * dismissing, while the panel keeps its own dismiss for every other exit.
 */
export const AuxiliaryPanelCloseOverrideContext =
  React.createContext<AuxiliaryPanelClose | null>(null);

export function requireAuxiliaryPanelContext(
  context: AuxiliaryPanelContextValue | null,
): AuxiliaryPanelContextValue {
  if (!context) {
    throw new Error("useAuxiliaryPanel must be used within AuxiliaryPanel");
  }

  return context;
}

export function resolveAuxiliaryPanelBodyMode({
  context,
  mode,
}: {
  context: AuxiliaryPanelContextValue | null;
  mode?: AuxiliaryPanelMode;
}): AuxiliaryPanelMode {
  const resolvedMode = mode ?? context?.mode;

  if (resolvedMode == null) {
    throw new Error(
      "AuxiliaryPanelBody requires `mode` or an AuxiliaryPanel ancestor",
    );
  }

  return resolvedMode;
}

/** Read chrome/layout state from the nearest `AuxiliaryPanel` ancestor. */
export function useAuxiliaryPanel(): AuxiliaryPanelContextValue {
  return requireAuxiliaryPanelContext(React.useContext(AuxiliaryPanelContext));
}
