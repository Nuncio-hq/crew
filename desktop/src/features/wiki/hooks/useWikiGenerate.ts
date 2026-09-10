import { validateWikiGenerationBatch } from "@/features/wiki/lib/wikiGenerationBatch";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";

import { wikiEventsQueryKey } from "@/features/wiki/hooks/useWikiEventsQuery";
import { setWikiJob } from "@/features/wiki/lib/wikiStore";
import { signRelayEvent } from "@/shared/api/tauri";
import { relayClient } from "@/shared/api/relayClient";
import { KIND_REPO_WIKI_PAGE } from "@/shared/constants/kinds";

type WikiDraft = {
  slug: string;
  title: string;
  section: string;
  sourceFiles: string[];
  commit: string;
  language: string;
  content: string;
  tags: string[][];
};

type WikiGenerateOutcome = {
  accepted: boolean;
  pages: number;
  commit: string;
  branch?: string;
  tocContent?: string;
  tocTags?: string[][];
  drafts?: WikiDraft[];
  emptyRepo?: boolean;
  missingLocalPath?: boolean;
  costNote?: string;
};

export function useWikiGenerate() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (input: {
      owner: string;
      repoD: string;
      repoKey: string;
      repoPath?: string | null;
    }) => {
      const costNote = "Heuristic generator · no API key billed";
      setWikiJob({
        repoKey: input.repoKey,
        status: "generating",
        done: 0,
        total: 0,
        error: null,
        costNote,
      });
      try {
        const outcome = await invoke<WikiGenerateOutcome>("wiki_generate", {
          owner: input.owner,
          repoD: input.repoD,
          repoPath: input.repoPath ?? null,
        });
        if (outcome.missingLocalPath) {
          setWikiJob({
            repoKey: input.repoKey,
            status: "idle",
            done: 0,
            total: 0,
            error: "missing-local-path",
            costNote: outcome.costNote ?? costNote,
          });
          return;
        }
        if (outcome.emptyRepo) {
          setWikiJob({
            repoKey: input.repoKey,
            status: "idle",
            done: 0,
            total: 0,
            error: "empty-repo",
            costNote: outcome.costNote ?? costNote,
          });
          return;
        }
        validateWikiGenerationBatch(outcome, input);
        const drafts = outcome.drafts ?? [];
        const total = Math.max(drafts.length, outcome.pages, 1);
        setWikiJob({
          repoKey: input.repoKey,
          status: "generating",
          done: 0,
          total,
          error: null,
          costNote: outcome.costNote ?? costNote,
        });
        if (outcome.tocContent && outcome.tocTags) {
          await publishWikiEvent(outcome.tocContent, outcome.tocTags);
        }
        let done = 0;
        for (const draft of drafts) {
          await publishWikiEvent(draft.content, draft.tags);
          done += 1;
          setWikiJob({
            repoKey: input.repoKey,
            status: "generating",
            done,
            total,
            error: null,
            costNote: outcome.costNote ?? costNote,
          });
          await queryClient.invalidateQueries({ queryKey: wikiEventsQueryKey });
        }
        setWikiJob({
          repoKey: input.repoKey,
          status: "idle",
          done: drafts.length || 1,
          total,
          error: null,
          costNote: outcome.costNote ?? null,
        });
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        setWikiJob({
          repoKey: input.repoKey,
          status: "failed",
          done: 0,
          total: 0,
          error: message,
          costNote,
        });
        throw error;
      }
      await queryClient.invalidateQueries({ queryKey: wikiEventsQueryKey });
    },
  });
}

async function publishWikiEvent(content: string, tags: string[][]) {
  const event = await signRelayEvent({
    kind: KIND_REPO_WIKI_PAGE,
    content,
    tags,
  });
  await relayClient.publishEvent(
    event,
    "Timed out publishing wiki event.",
    "Failed to publish wiki event.",
  );
}

export function useWikiSetCadence() {
  return useMutation({
    mutationFn: async (input: {
      owner: string;
      repoD: string;
      cadence: string;
      commit: string;
      branch: string;
      sectionsJson: string;
    }) => {
      await publishWikiEvent(input.sectionsJson, [
        ["d", `${input.repoD}/_toc`],
        ["a", `30617:${input.owner}:${input.repoD}`],
        ["commit", input.commit],
        ["branch", input.branch],
        ["cadence", input.cadence],
        ["title", "Wiki"],
      ]);
    },
  });
}
