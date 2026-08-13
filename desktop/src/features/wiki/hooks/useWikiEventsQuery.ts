import { useQuery } from "@tanstack/react-query";

import {
  parseCompanyWikiPage,
  parseWikiPage,
  parseWikiToc,
  type CompanyWikiPage,
  type WikiPage,
  type WikiToc,
} from "@/features/wiki/lib/wikiEvents";
import { relayClient } from "@/shared/api/relayClient";
import type { RelayEvent } from "@/shared/api/types";
import {
  KIND_LONG_FORM,
  KIND_REPO_STATE,
  KIND_REPO_WIKI_PAGE,
} from "@/shared/constants/kinds";

export const wikiEventsQueryKey = ["crew-wiki-events"] as const;

export function useWikiEventsQuery() {
  return useQuery({
    queryKey: wikiEventsQueryKey,
    queryFn: async (): Promise<{
      tocs: WikiToc[];
      pages: WikiPage[];
      company: CompanyWikiPage[];
      states: RelayEvent[];
    }> => {
      const wikiEvents = await relayClient.fetchEvents({
        kinds: [KIND_REPO_WIKI_PAGE],
        limit: 500,
      });
      const notes = await relayClient.fetchEvents({
        kinds: [KIND_LONG_FORM],
        limit: 200,
      });
      const states = await relayClient.fetchEvents({
        kinds: [KIND_REPO_STATE],
        limit: 200,
      });
      const tocs: WikiToc[] = [];
      const pages: WikiPage[] = [];
      for (const event of wikiEvents) {
        const toc = parseWikiToc(event);
        if (toc) {
          tocs.push(toc);
          continue;
        }
        const page = parseWikiPage(event);
        if (page) pages.push(page);
      }
      const company = notes
        .map(parseCompanyWikiPage)
        .filter((page): page is CompanyWikiPage => page !== null);
      return { tocs, pages, company, states };
    },
    staleTime: 10_000,
  });
}
