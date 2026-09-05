import * as React from "react";

import { ChannelToolPane } from "@/features/tool-pane/ChannelToolPane";
import { useToolPane } from "@/features/tool-pane/toolPaneStore";
import {
  clampAuxiliaryPanelWidth,
  getAuxiliaryPanelMaxWidth,
} from "@/shared/layout/auxiliaryPanelLayout";
import { cn } from "@/shared/lib/cn";
import { useThreadForgeHubSubject } from "@/features/messages/lib/threadForgeHubSubjectStore";

import { FORGE_HUB_NARROW_PX } from "./forgeHubCopy";

export function ThreadFocusForgeSplit({
  channelId,
  channelName,
  children,
  threadRootId,
}: {
  channelId: string | null;
  channelName: string;
  children: React.ReactNode;
  threadRootId: string | null;
}) {
  const subject = useThreadForgeHubSubject();
  const pane = useToolPane();
  const rootRef = React.useRef<HTMLDivElement>(null);
  const [width, setWidth] = React.useState(1400);
  const [chatPane, setChatPane] = React.useState<"chat" | "tools">("tools");

  React.useEffect(() => {
    void subject;
    const node = rootRef.current;
    if (!node) return;
    const observer = new ResizeObserver((entries) => {
      const next = entries[0]?.contentRect.width;
      if (next) setWidth(next);
    });
    observer.observe(node);
    setWidth(node.getBoundingClientRect().width);
    return () => observer.disconnect();
  }, [subject]);

  const showTools = Boolean(subject) || pane.open;
  if (!showTools || !channelId) {
    return <>{children}</>;
  }

  const narrow = width > 0 && width < FORGE_HUB_NARROW_PX;
  const viewport = typeof window === "undefined" ? width : window.innerWidth;
  const hubWidth = clampAuxiliaryPanelWidth(
    Math.round(width * 0.6),
    Math.max(viewport, getAuxiliaryPanelMaxWidth(viewport)),
  );

  return (
    <div
      className="flex min-h-0 min-w-0 flex-1 flex-col"
      data-testid="thread-forge-focus-split"
      ref={rootRef}
    >
      {narrow ? (
        <div
          className="flex shrink-0 items-center gap-1 border-b border-border/60 px-3 py-1.5"
          data-testid="thread-forge-pane-toggle"
        >
          <ToggleChip
            active={chatPane === "chat"}
            label="Chat"
            onClick={() => setChatPane("chat")}
          />
          <ToggleChip
            active={chatPane === "tools"}
            label={subject ? "PR" : "Tools"}
            onClick={() => setChatPane("tools")}
          />
        </div>
      ) : null}
      <div className="flex min-h-0 min-w-0 flex-1 flex-row">
        <div
          className={cn(
            "flex min-h-0 min-w-0 flex-col",
            narrow && chatPane !== "chat" ? "hidden" : "flex-1",
          )}
        >
          {children}
        </div>
        <div
          className={cn(
            "flex min-h-0 min-w-0 flex-col border-l border-border/60 bg-background",
            narrow && "flex-1",
            narrow && chatPane !== "tools" ? "hidden" : null,
          )}
          data-testid="thread-pr-hub"
          style={narrow ? undefined : { width: hubWidth }}
        >
          <ChannelToolPane
            channelId={channelId}
            channelName={channelName}
            forgeSubject={subject}
            mode="thread"
            threadRootId={threadRootId}
            worktreePath={subject?.worktreePath}
          />
        </div>
      </div>
    </div>
  );
}

function ToggleChip({
  active,
  label,
  onClick,
}: {
  active: boolean;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      className={cn(
        "rounded-md px-2 py-0.5 text-sm",
        active
          ? "bg-muted font-semibold text-foreground"
          : "text-muted-foreground hover:text-foreground",
      )}
      onClick={onClick}
      type="button"
    >
      {label}
    </button>
  );
}
