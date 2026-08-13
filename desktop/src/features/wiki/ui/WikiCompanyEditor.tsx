import * as React from "react";

import { useQueryClient } from "@tanstack/react-query";

import { wikiEventsQueryKey } from "@/features/wiki/hooks/useWikiEventsQuery";
import type { CompanyWikiPage } from "@/features/wiki/lib/wikiEvents";
import { relayClient } from "@/shared/api/relayClient";
import { signRelayEvent } from "@/shared/api/tauri";
import { KIND_LONG_FORM } from "@/shared/constants/kinds";
import { Button } from "@/shared/ui/button";
import { cn } from "@/shared/lib/cn";

export function WikiCompanyEditor({
  proposals,
  className,
}: {
  proposals: CompanyWikiPage[];
  className?: string;
}) {
  const queryClient = useQueryClient();
  const [title, setTitle] = React.useState("");
  const [slug, setSlug] = React.useState("welcome");
  const [body, setBody] = React.useState("");

  const publish = async (
    pageSlug: string,
    pageTitle: string,
    content: string,
  ) => {
    const event = await signRelayEvent({
      kind: KIND_LONG_FORM,
      content,
      tags: [
        ["d", pageSlug],
        ["title", pageTitle],
      ],
    });
    await relayClient.publishEvent(
      event,
      "Timed out publishing company wiki page.",
      "Failed to publish company wiki page.",
    );
    await queryClient.invalidateQueries({ queryKey: wikiEventsQueryKey });
  };

  return (
    <div className={cn("rounded-xl border border-border p-4", className)}>
      <h3 className="mb-2 text-sm font-medium">Create company page</h3>
      <input
        aria-label="Company page title"
        className="mb-2 h-8 w-full rounded-md border border-border bg-background px-2 text-sm"
        data-testid="wiki-company-title"
        onChange={(event) => setTitle(event.target.value)}
        placeholder="Title"
        value={title}
      />
      <input
        aria-label="Company page slug"
        className="mb-2 h-8 w-full rounded-md border border-border bg-background px-2 font-mono text-sm"
        onChange={(event) => setSlug(event.target.value)}
        placeholder="slug"
        value={slug}
      />
      <textarea
        aria-label="Company page body"
        className="mb-2 min-h-24 w-full rounded-md border border-border bg-background p-2 text-sm"
        data-testid="wiki-company-body"
        onChange={(event) => setBody(event.target.value)}
        value={body}
      />
      <Button
        data-testid="wiki-company-publish"
        disabled={!title.trim() || !body.trim()}
        onClick={() => void publish(slug, title, body)}
        size="sm"
      >
        Publish
      </Button>
      {proposals.length > 0 ? (
        <div className="mt-4" data-testid="wiki-proposal-queue">
          <h4 className="mb-2 text-sm font-medium">Agent proposals</h4>
          {proposals.map((proposal) => (
            <div
              className="mb-2 rounded-md border border-border p-2"
              key={proposal.event.id}
            >
              <div className="text-sm">{proposal.title}</div>
              {proposal.engramSlug ? (
                <div className="text-2xs text-muted-foreground">
                  from {proposal.engramSlug}
                </div>
              ) : null}
              <Button
                className="mt-2"
                data-testid={`wiki-proposal-accept-${proposal.slug.replace(/^_proposal\//, "")}`}
                onClick={() =>
                  void publish(
                    proposal.slug.replace(/^_proposal\//, ""),
                    proposal.title,
                    proposal.content,
                  )
                }
                size="sm"
                variant="outline"
              >
                Review and publish
              </Button>
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}
