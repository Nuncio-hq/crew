import * as React from "react";
import { createPortal } from "react-dom";
import { MessageSquarePlus } from "lucide-react";

import { normalizeAddToChatSelection } from "@/features/messages/lib/addToChatQuote";
import { useAddToChat } from "./AddToChatContext";

type SelectionAction = { left: number; text: string; top: number };

export function MessageSelectionAddToChat({
  children,
  enabled,
}: {
  children: React.ReactNode;
  enabled: boolean;
}) {
  const context = useAddToChat();
  const rootRef = React.useRef<HTMLDivElement>(null);
  const [action, setAction] = React.useState<SelectionAction | null>(null);

  const captureSelection = React.useCallback(() => {
    const selection = window.getSelection();
    const root = rootRef.current;
    if (!selection || selection.isCollapsed || !root || !selection.rangeCount) {
      setAction(null);
      return;
    }
    const range = selection.getRangeAt(0);
    if (!root.contains(range.commonAncestorContainer)) {
      setAction(null);
      return;
    }
    const text = normalizeAddToChatSelection(selection.toString());
    if (!text) {
      setAction(null);
      return;
    }
    const rect = range.getBoundingClientRect();
    setAction({
      left: Math.min(
        window.innerWidth - 76,
        Math.max(8, rect.left + rect.width / 2),
      ),
      text,
      top: Math.max(40, rect.top - 8),
    });
  }, []);

  React.useEffect(() => {
    if (!action) return;
    const dismiss = () => setAction(null);
    const dismissCollapsedSelection = () => {
      const selection = window.getSelection();
      if (!selection || selection.isCollapsed) dismiss();
    };
    document.addEventListener("selectionchange", dismissCollapsedSelection);
    window.addEventListener("scroll", dismiss, true);
    window.addEventListener("resize", dismiss);
    return () => {
      document.removeEventListener(
        "selectionchange",
        dismissCollapsedSelection,
      );
      window.removeEventListener("scroll", dismiss, true);
      window.removeEventListener("resize", dismiss);
    };
  }, [action]);

  if (!enabled || !context) return <>{children}</>;

  return (
    <div
      ref={rootRef}
      onPointerUp={() => requestAnimationFrame(captureSelection)}
    >
      {children}
      {action
        ? createPortal(
            <button
              aria-label="Add selected text to chat"
              className="fixed z-[100] flex -translate-x-1/2 -translate-y-full items-center gap-1.5 rounded-md border border-border bg-popover px-2.5 py-1.5 text-xs font-medium text-popover-foreground shadow-md hover:bg-muted focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
              data-testid="add-selection-to-chat"
              onPointerDown={(event) => event.preventDefault()}
              onClick={() => {
                if (context.addSelection(action.text)) {
                  window.getSelection()?.removeAllRanges();
                  setAction(null);
                }
              }}
              style={{ left: action.left, top: action.top }}
              type="button"
            >
              <MessageSquarePlus aria-hidden className="size-3.5" />
              Add to Chat
            </button>,
            document.body,
          )
        : null}
    </div>
  );
}
