import * as React from "react";

import { getThreadForgeViewContext } from "@/features/messages/lib/threadForgeViewContextStore";

import {
  closeToolPane,
  openThreadToolPane,
  openToolPane,
} from "./toolPaneStore";

export function useToolPaneShortcuts(channelId: string | null) {
  React.useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (event.defaultPrevented || event.altKey || event.isComposing) return;
      const meta = event.metaKey || event.ctrlKey;
      if (meta && event.shiftKey && event.code === "KeyB") {
        if (!channelId) return;
        event.preventDefault();
        openTools("browser");
        return;
      }
      if (meta && event.shiftKey && event.code === "KeyM") {
        if (!channelId) return;
        event.preventDefault();
        openTools("sim");
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
    function openTools(tab: "browser" | "sim") {
      const thread = getThreadForgeViewContext();
      if (
        thread?.channelId === channelId &&
        typeof thread.rootEventId === "string" &&
        thread.rootEventId.length > 0
      ) {
        openThreadToolPane(tab);
      } else {
        openToolPane(tab);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [channelId]);
}
