import * as React from "react";

import { setWorkTreeDisclosure } from "../lib/workTreeDisclosure";

function visibleTreeButtons(root: HTMLElement): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>("button")].filter((button) => {
    const testId = button.dataset.testid ?? "";
    return (
      testId.startsWith("work-tree-disclosure-") ||
      testId.startsWith("channel-") ||
      testId.startsWith("work-thread-row-") ||
      testId.startsWith("work-tree-more-") ||
      testId === "needs-you-header" ||
      testId.startsWith("needs-you-item-")
    );
  });
}

export function useWorkTreeKeyboard(
  rootRef: React.RefObject<HTMLDivElement | null>,
) {
  React.useEffect(() => {
    const root = rootRef.current;
    if (!root) return undefined;
    const onKeyDown = (event: KeyboardEvent) => {
      const activeEl = document.activeElement;
      if (!(activeEl instanceof HTMLElement) || !root.contains(activeEl)) {
        return;
      }
      const buttons = visibleTreeButtons(root);
      const index = buttons.indexOf(activeEl);
      if (event.key === "ArrowDown") {
        event.preventDefault();
        buttons[Math.min(index + 1, buttons.length - 1)]?.focus();
        return;
      }
      if (event.key === "ArrowUp") {
        event.preventDefault();
        buttons[Math.max(index - 1, 0)]?.focus();
        return;
      }
      if (event.key === "ArrowRight" || event.key === "ArrowLeft") {
        const active = document.activeElement;
        if (!(active instanceof HTMLElement)) return;
        const folder = active.closest(
          "[data-testid^='work-tree-folder-block-']",
        );
        const channelId = folder
          ?.getAttribute("data-testid")
          ?.replace("work-tree-folder-block-", "");
        if (!channelId) return;
        event.preventDefault();
        setWorkTreeDisclosure(channelId, {
          expanded: event.key === "ArrowRight",
        });
      }
    };
    root.addEventListener("keydown", onKeyDown);
    return () => root.removeEventListener("keydown", onKeyDown);
  }, [rootRef]);
}
