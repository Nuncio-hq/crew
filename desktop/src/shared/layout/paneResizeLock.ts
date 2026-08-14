/**
 * Disable CSS transitions/animations for the duration of a pane-handle drag
 * (#205). Width must snap with the pointer; motion resumes on release.
 */

const DATASET_KEY = "paneResizing";

export function lockPaneResizeMotion(): void {
  if (typeof document === "undefined") return;
  document.documentElement.dataset[DATASET_KEY] = "true";
}

export function unlockPaneResizeMotion(): void {
  if (typeof document === "undefined") return;
  delete document.documentElement.dataset[DATASET_KEY];
}
