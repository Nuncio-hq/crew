import { useQueryClient } from "@tanstack/react-query";

import { wikiEventsQueryKey } from "@/features/wiki/hooks/useWikiEventsQuery";
import type { CompanyWikiPage } from "@/features/wiki/lib/wikiEvents";
import { relayClient } from "@/shared/api/relayClient";
import { signRelayEvent } from "@/shared/api/tauri";
import { KIND_LONG_FORM } from "@/shared/constants/kinds";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";

/**
 * Owner review of agent-proposed company pages. Wiki is not a CMS — there
 * is no "Create company page" form here (#221).
 */
export function WikiCompanyEditor({
  proposals,
  className,
}: {
  proposals: CompanyWikiPage[];
  className?: string;
}) {
  const queryClient = useQueryClient();

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

  if (proposals.length === 0) {
    return null;
  }

  return (
    <div
      className={cn("rounded-xl border border-border bg-card p-4", className)}
      data-testid="wiki-proposal-queue"
    >
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
  );
}
