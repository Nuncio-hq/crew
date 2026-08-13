import { SquareArrowOutUpRight, X } from "lucide-react";
import * as React from "react";

import type { ThreadForgeHubSubject } from "@/features/messages/lib/threadForgeHubSubjectStore";
import { ThreadPrHub } from "@/features/messages/ui/threadPrHub/ThreadPrHub";
import { cn } from "@/shared/lib/cn";

import { BrowserTab } from "./BrowserTab";
import { GovernorStrip } from "./GovernorStrip";
import { getCanvasTooling, openToolPaneWindow } from "./governorClient";
import { SimTab } from "./SimTab";
import {
  closeToolPane,
  setToolPanePoppedOut,
  setToolPaneTab,
  useToolPane,
} from "./toolPaneStore";
import type { CanvasTooling, ToolPaneTab } from "./types";

export function ChannelToolPane({
  channelId,
  channelName,
  checkoutPath,
  forgeSubject,
  mode,
  threadRootId,
  worktreePath,
}: {
  channelId: string;
  channelName: string;
  checkoutPath?: string | null;
  forgeSubject?: ThreadForgeHubSubject | null;
  mode: "channel" | "thread";
  threadRootId?: string | null;
  worktreePath?: string | null;
}) {
  const pane = useToolPane();
  const [tooling, setTooling] = React.useState<CanvasTooling | null>(null);
  const showPr = Boolean(forgeSubject);
  const tabs: ToolPaneTab[] =
    mode === "thread" && showPr ? ["pr", "browser", "sim"] : ["sim", "browser"];
  const active: ToolPaneTab = tabs.includes(pane.tab) ? pane.tab : tabs[0];

  React.useEffect(() => {
    void getCanvasTooling(channelId)
      .then(setTooling)
      .catch(() => setTooling(null));
  }, [channelId]);

  return (
    <div
      className="flex min-h-0 min-w-0 flex-1 flex-col bg-background"
      data-channel-id={channelId}
      data-mode={mode}
      data-popped-out={pane.poppedOut ? "true" : "false"}
      data-testid="channel-tool-pane"
    >
      <div
        className="flex shrink-0 items-center gap-1 border-b border-border/60 px-2 py-1.5"
        data-testid="tool-pane-tabs"
      >
        {tabs.map((tab) => (
          <TabChip
            active={active === tab}
            key={tab}
            label={tabLabel(tab)}
            onClick={() => setToolPaneTab(tab)}
            testId={`tool-pane-tab-${tab}`}
          />
        ))}
        <button
          aria-label="Pop out Tools"
          className="ml-auto rounded-md p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
          data-testid="tool-pane-popout"
          onClick={() => {
            setToolPanePoppedOut(true);
            void openToolPaneWindow(channelId).catch(() => undefined);
          }}
          type="button"
        >
          <SquareArrowOutUpRight className="h-3.5 w-3.5" />
        </button>
        <button
          aria-label="Close Tools"
          className="rounded-md p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
          data-testid="tool-pane-close"
          onClick={() => {
            if (forgeSubject) {
              setToolPaneTab("pr");
              return;
            }
            closeToolPane();
          }}
          type="button"
        >
          <X className="h-3.5 w-3.5" />
        </button>
      </div>
      <div className="flex min-h-0 flex-1 flex-col">
        {active === "pr" && forgeSubject ? (
          <div
            className="min-h-0 flex-1 overflow-auto"
            data-testid="thread-pr-hub"
          >
            <ThreadPrHub subject={forgeSubject} />
          </div>
        ) : null}
        {active === "browser" ? (
          <BrowserTab
            channelId={channelId}
            channelName={channelName}
            checkoutPath={checkoutPath}
            threadRootId={threadRootId}
            worktreePath={worktreePath}
          />
        ) : null}
        {active === "sim" ? (
          <SimTab
            channelId={channelId}
            channelName={channelName}
            threadRootId={threadRootId}
            tooling={tooling}
          />
        ) : null}
      </div>
      <GovernorStrip />
    </div>
  );
}

function TabChip({
  active,
  label,
  onClick,
  testId,
}: {
  active: boolean;
  label: string;
  onClick: () => void;
  testId: string;
}) {
  return (
    <button
      className={cn(
        "rounded-md px-2 py-0.5 text-sm",
        active
          ? "bg-muted font-semibold text-foreground"
          : "text-muted-foreground hover:text-foreground",
      )}
      data-testid={testId}
      onClick={onClick}
      type="button"
    >
      {label}
    </button>
  );
}

function tabLabel(tab: ToolPaneTab): string {
  switch (tab) {
    case "pr":
      return "PR";
    case "browser":
      return "Browser";
    case "sim":
      return "Sim";
    default: {
      const exhaustive: never = tab;
      return exhaustive;
    }
  }
}
