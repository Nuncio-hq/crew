/**
 * Tracks closable foreground surfaces currently listening for Escape.
 *
 * Escape has app-wide meaning (mark channel read) *and* surface-local meaning
 * (close the panel above the channel). Window listeners fire in registration
 * order, so the app-level shortcut — registered at mount — would otherwise
 * always win the key over a panel that opened later. Instead of racing,
 * background shortcuts ask "is any closable surface open?" and yield.
 *
 * Nested controls (autocomplete, edit mode) still take priority over the
 * surfaces themselves: they handle Escape on the element and mark it
 * `defaultPrevented`, which every surface listener already respects.
 */
let activeEscapeSurfaceCount = 0;

/** True while at least one closable surface is listening for Escape. */
export function hasActiveEscapeSurface(): boolean {
  return activeEscapeSurfaceCount > 0;
}

/**
 * Registers a closable surface. Returns a release function that must be
 * called exactly once when the surface stops listening (idempotent — extra
 * calls are ignored so a double-cleanup cannot corrupt the count).
 */
export function acquireEscapeSurface(): () => void {
  activeEscapeSurfaceCount += 1;
  let released = false;
  return () => {
    if (released) return;
    released = true;
    activeEscapeSurfaceCount -= 1;
  };
}

/**
 * Marks an element that handles Escape itself.
 *
 * Surfaces that claim Escape in the *capture* phase (the focus thread drawer)
 * decide before the element ever sees the key, so `defaultPrevented` cannot
 * tell them a nested control owns it. The marker is the capture-phase form of
 * the same contract: a listener that stops propagation must first ask whether
 * the key landed inside a control that claims it.
 */
export const ESCAPE_OWNER_ATTRIBUTE = "data-escape-owner";

/** True when the event landed inside an element marked as an Escape owner. */
export function escapeIsClaimedByNestedOwner(target: EventTarget | null) {
  if (!target || typeof (target as Element).closest !== "function")
    return false;
  return (target as Element).closest(`[${ESCAPE_OWNER_ATTRIBUTE}]`) !== null;
}
