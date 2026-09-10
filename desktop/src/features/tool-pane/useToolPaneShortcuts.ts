import * as React from "react";

import { closeToolPane, openToolPane } from "./toolPaneStore";

export function useToolPaneShortcuts(channelId: string | null) {
  React.useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (event.defaultPrevented || event.altKey || event.isComposing) return;
      const meta = event.metaKey || event.ctrlKey;
      if (meta && event.shiftKey && event.code === "KeyB") {
        if (!channelId) return;
        event.preventDefault();
        openToolPane("browser");
        return;
      }
      if (meta && event.shiftKey && event.code === "KeyM") {
        if (!channelId) return;
        event.preventDefault();
        openToolPane("sim");
        return;
      }
      if (event.key === "Escape" && !event.metaKey && !event.ctrlKey) {
        const target = event.target as HTMLElement | null;
        const tag = target?.tagName;
        if (
          tag === "INPUT" ||
          tag === "TEXTAREA" ||
          target?.isContentEditable
        ) {
          return;
        }
        closeToolPane();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [channelId]);
}
