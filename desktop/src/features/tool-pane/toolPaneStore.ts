import * as React from "react";

import type { ToolPaneTab } from "./types";

type Snapshot = {
  open: boolean;
  tab: ToolPaneTab;
  poppedOut: boolean;
};

let snapshot: Snapshot = { open: false, tab: "sim", poppedOut: false };
const listeners = new Set<() => void>();

function publish(next: Snapshot) {
  snapshot = next;
  for (const listener of listeners) listener();
}

export function openToolPane(tab?: ToolPaneTab) {
  publish({
    ...snapshot,
    open: true,
    tab: tab ?? snapshot.tab,
  });
}

export function closeToolPane() {
  if (!snapshot.open && !snapshot.poppedOut) return;
  publish({ ...snapshot, open: false, poppedOut: false });
}

export function dismiss(onDismiss: () => void, keepToolPane: boolean) {
  if (!keepToolPane) closeToolPane();
  onDismiss();
}

export function toggleToolPane(tab?: ToolPaneTab) {
  if (snapshot.open && (!tab || tab === snapshot.tab)) {
    closeToolPane();
    return;
  }
  openToolPane(tab);
}

export function setToolPaneTab(tab: ToolPaneTab) {
  publish({ ...snapshot, open: true, tab });
}

export function setToolPanePoppedOut(poppedOut: boolean) {
  publish({ ...snapshot, poppedOut, open: poppedOut ? true : snapshot.open });
}

export function useToolPane() {
  return React.useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => snapshot,
  );
}

export function resetToolPaneForTests() {
  snapshot = { open: false, tab: "sim", poppedOut: false };
}

export function getToolPaneSnapshot() {
  return snapshot;
}
