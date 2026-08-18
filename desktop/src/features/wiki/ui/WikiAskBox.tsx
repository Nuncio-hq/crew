import * as React from "react";

import { useAppNavigation } from "@/app/navigation/useAppNavigation";
import { saveDraftEntry } from "@/features/messages/lib/useDrafts";
import { useChannelsQuery } from "@/features/channels/hooks";
import { ask, type AskMode } from "@/features/wiki/lib/wikiAsk";
import { buildFileLink, parseEntityLink } from "@/shared/lib/entityLink";
import { useOpenEntityLink } from "@/shared/ui/markdown/entityLinks";
import {
  OFFICE_COMPOSER_SURFACE_CLASS,
  OFFICE_FIELD_BOX_CLASS,
  OFFICE_FIELD_CONTROL_CLASS,
  OFFICE_SURFACE,
} from "@/shared/layout/officeChrome";
import { Button } from "@/shared/ui/button";
import { cn } from "@/shared/lib/cn";

export function WikiAskBox({
  channelId,
  door,
  owner,
  repoD,
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
  const [answer, setAnswer] = React.useState<ReturnType<typeof ask> | null>(
    null,
  );
  const [pickedChannel, setPickedChannel] = React.useState(channelId ?? "");
  const channels = useChannelsQuery();
  const { goChannel } = useAppNavigation();
  const openLink = useOpenEntityLink();

  const onAsk = () => {
    if (!question.trim()) return;
    setAnswer(
      ask({
        question,
        mode,
        repoD,
        hits: [
          {
            title: "README.md#L1-12",
            excerpt:
              "Crew Wiki grounds answers in generated repo pages and company notes.",
            href:
              repoD && owner && owner.length === 64
                ? buildFileLink({
                    owner,
                    dtag: repoD,
                    path: "desktop/src/features/projects/ui/ProjectDetailScreen.tsx",
                    lines: "1-3",
                  })
                : undefined,
          },
        ],
      }),
    );
  };

  return (
    <div className="shrink-0 px-4 pb-3 pt-2">
      <div
        className={OFFICE_COMPOSER_SURFACE_CLASS}
        data-office-surface={OFFICE_SURFACE.composerSurface}
        data-testid="wiki-ask"
      >
        <div className="mb-1 text-2xs text-muted-foreground">{scopeLabel}</div>
        {answer ? (
          <div
            className="mb-2 rounded-md border border-border bg-muted/20 p-3 text-sm"
            data-testid="wiki-ask-answer"
          >
            <div className="whitespace-pre-wrap">{answer.markdown}</div>
            {answer.citations.map((citation) =>
              citation.href ? (
                <a
                  className="mr-1 mt-1 inline-block rounded bg-muted px-1 font-mono text-2xs text-primary"
                  href={citation.href}
                  key={citation.href}
                  onClick={(event) => {
                    event.preventDefault();
                    const parsed = parseEntityLink(citation.href);
                    if (parsed.ok) openLink(parsed.value);
                  }}
                >
                  {citation.label}
                </a>
              ) : (
                <span
                  className="mr-1 mt-1 inline-block rounded bg-muted px-1 font-mono text-2xs"
                  key={citation.label}
                >
                  {citation.label}
                </span>
              ),
            )}
            {answer.mode === "plan" ? (
              <div className="mt-3 flex items-center gap-2">
                {door === "library" ? (
                  <select
                    aria-label="Plan channel"
                    className={cn(
                      OFFICE_FIELD_BOX_CLASS,
                      OFFICE_FIELD_CONTROL_CLASS,
                      "h-8 px-2 text-2xs",
                    )}
                    data-testid="wiki-plan-channel"
                    onChange={(event) => setPickedChannel(event.target.value)}
                    value={pickedChannel}
                  >
                    <option value="">Pick channel</option>
                    {(channels.data ?? []).map((channel) => (
                      <option key={channel.id} value={channel.id}>
                        {channel.name}
                      </option>
                    ))}
                  </select>
                ) : null}
                <Button
                  data-testid="wiki-start-thread"
                  disabled={
                    !((channelId || pickedChannel) && answer.threadDraft)
                  }
                  onClick={() => {
                    const target = channelId || pickedChannel;
                    if (!target || !answer.threadDraft) return;
                    const now = new Date().toISOString();
                    saveDraftEntry(target, {
                      content: answer.threadDraft,
                      selectionStart: 0,
                      selectionEnd: 0,
                      channelId: target,
                      createdAt: now,
                      updatedAt: now,
                      pendingImeta: [],
                      spoileredAttachmentUrls: [],
                      status: "active",
                    });
                    void goChannel(target);
                  }}
                  size="sm"
                >
                  Start thread with this plan
                </Button>
              </div>
            ) : null}
          </div>
        ) : null}
        <div className="flex items-center gap-2">
          <select
            aria-label="Ask mode"
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
            className={cn(
              OFFICE_FIELD_BOX_CLASS,
              OFFICE_FIELD_CONTROL_CLASS,
              "h-8 min-w-0 flex-1 px-2 text-sm",
            )}
            data-testid="wiki-ask-input"
            onChange={(event) => setQuestion(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") onAsk();
            }}
            placeholder="Ask about this wiki"
            value={question}
          />
          <Button onClick={onAsk} size="sm" type="button">
            Ask
          </Button>
        </div>
      </div>
    </div>
  );
}
