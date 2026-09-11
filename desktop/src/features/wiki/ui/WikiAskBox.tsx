import * as React from "react";

import {
  WIKI_ASK_UNAVAILABLE_REASON,
  type AskMode,
} from "@/features/wiki/lib/wikiAsk";
import {
  OFFICE_COMPOSER_SURFACE_CLASS,
  OFFICE_FIELD_BOX_CLASS,
  OFFICE_FIELD_CONTROL_CLASS,
  OFFICE_SURFACE,
} from "@/shared/layout/officeChrome";
import { Button } from "@/shared/ui/button";
import { cn } from "@/shared/lib/cn";

export function WikiAskBox({
  scopeLabel,
}: {
  channelId?: string | null;
  door: "library" | "project";
  owner?: string;
  repoD?: string;
  scopeLabel: string;
}) {
  const [mode, setMode] = React.useState<AskMode>("auto");
  const [question, setQuestion] = React.useState("");

  return (
    <div className="shrink-0 px-4 pb-3 pt-2">
      <div
        className={OFFICE_COMPOSER_SURFACE_CLASS}
        data-office-surface={OFFICE_SURFACE.composerSurface}
        data-testid="wiki-ask"
      >
        <div className="mb-1 text-2xs text-muted-foreground">{scopeLabel}</div>
        <p
          aria-live="polite"
          className="mb-2 rounded-md border border-border bg-muted/20 p-2 text-2xs text-muted-foreground"
          data-testid="wiki-ask-unavailable"
          id="wiki-ask-unavailable"
          role="status"
        >
          {WIKI_ASK_UNAVAILABLE_REASON}
        </p>
        <div className="flex items-center gap-2">
          <select
            aria-label="Ask mode"
            aria-describedby="wiki-ask-unavailable"
            className={cn(
              OFFICE_FIELD_BOX_CLASS,
              OFFICE_FIELD_CONTROL_CLASS,
              "h-8 px-2 text-2xs",
            )}
            data-testid="wiki-ask-mode"
            onChange={(event) => setMode(event.target.value as AskMode)}
            value={mode}
          >
            <option value="auto">Auto</option>
            <option value="qa">Q&A</option>
            <option value="plan">Plan</option>
          </select>
          <input
            aria-label="Ask the wiki"
            aria-describedby="wiki-ask-unavailable"
            className={cn(
              OFFICE_FIELD_BOX_CLASS,
              OFFICE_FIELD_CONTROL_CLASS,
              "h-8 min-w-0 flex-1 px-2 text-sm",
            )}
            data-testid="wiki-ask-input"
            onChange={(event) => setQuestion(event.target.value)}
            onKeyDown={(event) => {
              if (event.key !== "Enter" || event.nativeEvent.isComposing) {
                return;
              }
              event.preventDefault();
            }}
            placeholder="Ask about this wiki"
            value={question}
          />
          <Button
            aria-describedby="wiki-ask-unavailable"
            disabled
            size="sm"
            type="button"
          >
            Ask
          </Button>
        </div>
      </div>
    </div>
  );
}
