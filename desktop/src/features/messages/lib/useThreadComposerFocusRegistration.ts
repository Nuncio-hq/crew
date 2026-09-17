import * as React from "react";
import { registerThreadComposerFocus } from "./threadComposerFocusRegistry";

/**
 * Registers a thread composer's own focus function under its `threadRootId`
 * so surfaces outside the composer (e.g. `LiveJobDesk`'s Steer button) can
 * focus it without a DOM selector. No-ops outside a thread audience context.
 */
export function useThreadComposerFocusRegistration(
  threadRootId: string | null | undefined,
  focus: () => void,
) {
  const focusRef = React.useRef(focus);
  focusRef.current = focus;
  React.useEffect(() => {
    if (!threadRootId) return;
    return registerThreadComposerFocus(threadRootId, () => focusRef.current());
  }, [threadRootId]);
}
