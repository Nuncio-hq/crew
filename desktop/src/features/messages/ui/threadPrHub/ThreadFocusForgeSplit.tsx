import * as React from "react";
import { ChannelToolPane } from "@/features/tool-pane/ChannelToolPane";
import {
  closeToolPane,
  getToolPaneOpener,
  isCurrentThreadToolPaneView,
  useToolPane,
} from "@/features/tool-pane/toolPaneStore";
import { useThreadToolPaneScope } from "@/features/tool-pane/useThreadToolPaneScope";
import { matchingThreadToolPaneSubject } from "@/features/tool-pane/threadToolPaneSubject";
import {
  clampAuxiliaryPanelWidth,
  getAuxiliaryPanelMaxWidth,
} from "@/shared/layout/auxiliaryPanelLayout";
import { useThreadForgeHubSubject } from "@/features/messages/lib/threadForgeHubSubjectStore";
import { Dialog, DialogContent, DialogTitle } from "@/shared/ui/dialog";
import { FORGE_HUB_NARROW_PX } from "./forgeHubCopy";

/** One explicit tool presentation; the conversation keeps its DOM and draft. */
export function ThreadFocusForgeSplit({
  channelId,
  channelName,
  children,
  threadRootId,
  onOverlayOpenChange,
}: {
  channelId: string | null;
  channelName: string;
  children: React.ReactNode;
  threadRootId: string | null;
  onOverlayOpenChange?: (open: boolean) => void;
}) {
  const subject = matchingThreadToolPaneSubject(
    useThreadForgeHubSubject(),
    channelId,
    threadRootId,
  );
  const pane = useToolPane();
  const scope = useThreadToolPaneScope(channelId, threadRootId);
  const rootRef = React.useRef<HTMLDivElement>(null);
  const wasOpenRef = React.useRef(false);
  const [width, setWidth] = React.useState(1400);
  const showTools = pane.open && isCurrentThreadToolPaneView(scope);
  const narrow = width > 0 && width < FORGE_HUB_NARROW_PX;
  const overlayOpen = showTools && narrow;

  React.useLayoutEffect(
    () => () => {
      if (isCurrentThreadToolPaneView(scope)) closeToolPane();
    },
    [scope],
  );

  React.useLayoutEffect(() => {
    const node = rootRef.current;
    if (!node) return;
    const observer = new ResizeObserver((entries) => {
      const next = entries[0]?.contentRect.width;
      if (next) setWidth(next);
    });
    observer.observe(node);
    setWidth(node.getBoundingClientRect().width);
    return () => observer.disconnect();
  }, []);
  React.useLayoutEffect(() => {
    onOverlayOpenChange?.(overlayOpen);
    return () => onOverlayOpenChange?.(false);
  }, [onOverlayOpenChange, overlayOpen]);
  React.useLayoutEffect(() => {
    if (
      wasOpenRef.current &&
      !showTools &&
      !narrow &&
      isCurrentThreadToolPaneView(scope)
    ) {
      const opener = getToolPaneOpener();
      const target = opener?.isConnected
        ? opener
        : rootRef.current?.querySelector<HTMLElement>(
            '[aria-label="Open thread tools"]',
          );
      target?.focus({ preventScroll: true });
    }
    wasOpenRef.current = showTools;
  }, [narrow, scope, showTools]);

  const viewport = typeof window === "undefined" ? width : window.innerWidth;
  const hubWidth = clampAuxiliaryPanelWidth(
    Math.round(width * 0.6),
    Math.max(viewport, getAuxiliaryPanelMaxWidth(viewport)),
  );
  const toolContent =
    showTools && channelId ? (
      <ChannelToolPane
        channelId={channelId}
        channelName={channelName}
        forgeSubject={subject}
        mode="thread"
        threadRootId={threadRootId}
        worktreePath={subject?.worktreePath}
        key={JSON.stringify(scope)}
      />
    ) : null;

  return (
    <div
      className="flex min-h-0 min-w-0 flex-1 flex-row"
      data-testid="thread-forge-focus-split"
      ref={rootRef}
    >
      <div
        className="flex min-h-0 min-w-0 flex-1 flex-col"
        inert={overlayOpen || undefined}
        aria-hidden={overlayOpen || undefined}
      >
        {children}
      </div>
      {showTools && !narrow ? (
        <div
          className="flex min-h-0 min-w-0 flex-col border-l border-border/60 bg-background"
          data-testid="thread-tools-pane"
          style={{ width: hubWidth }}
        >
          {toolContent}
        </div>
      ) : null}
      <Dialog
        open={overlayOpen}
        onOpenChange={(open) => {
          if (!open && isCurrentThreadToolPaneView(scope)) closeToolPane();
        }}
      >
        <DialogContent
          aria-describedby={undefined}
          className="flex h-[calc(100dvh-2rem)] flex-col gap-0 overflow-hidden p-0"
          showCloseButton={false}
          onEscapeKeyDown={(event) => {
            if (
              event.isComposing ||
              event.altKey ||
              event.ctrlKey ||
              event.metaKey ||
              event.shiftKey
            )
              event.preventDefault();
          }}
          onCloseAutoFocus={(event) => {
            event.preventDefault();
            const opener = getToolPaneOpener();
            const fallback = rootRef.current?.querySelector<HTMLElement>(
              '[aria-label="Open thread tools"]',
            );
            (opener?.isConnected && isCurrentThreadToolPaneView(scope)
              ? opener
              : fallback
            )?.focus({ preventScroll: true });
          }}
        >
          <DialogTitle className="sr-only">Thread tools</DialogTitle>
          {narrow ? toolContent : null}
        </DialogContent>
      </Dialog>
    </div>
  );
}
