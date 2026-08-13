import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { describe, it } from "node:test";
import { fileURLToPath } from "node:url";

import React from "react";
import { renderToStaticMarkup } from "react-dom/server";

import { ChannelUserInputCard as ChannelCard } from "../../channels/ui/ChannelUserInputCard.tsx";
import { MessageRowDefaultBody } from "../../messages/ui/MessageRowDefaultBody.tsx";
import { ChannelUserInputCard as WorkbenchCard } from "./workbenchSharedRenderers.ts";
import { MessageRowDefaultBody as WorkbenchBody } from "./workbenchSharedRenderers.ts";

const here = dirname(fileURLToPath(import.meta.url));

describe("workbench component-reuse contract (#186)", () => {
  it("re-exports the channel/session components instead of forking copies", () => {
    const source = readFileSync(
      join(here, "workbenchSharedRenderers.ts"),
      "utf8",
    );
    assert.match(
      source,
      /export \{ MessageRow \} from "@\/features\/messages\/ui\/MessageRow"/,
    );
    assert.match(
      source,
      /export \{ EvidenceCard \} from "@\/features\/messages\/ui\/EvidenceCard"/,
    );
    assert.match(
      source,
      /export \{ ChannelUserInputCard \} from "@\/features\/channels\/ui\/ChannelUserInputCard"/,
    );
    assert.match(
      source,
      /export \{ ProjectThreadGitHubRow \} from "@\/features\/messages\/ui\/ProjectThreadGitHubRow"/,
    );
    assert.equal(WorkbenchCard, ChannelCard);
    assert.equal(WorkbenchBody, MessageRowDefaultBody);
    const transcriptSource = readFileSync(
      join(here, "../ui/WorkbenchTranscript.tsx"),
      "utf8",
    );
    assert.match(
      transcriptSource,
      /from "\.\.\/lib\/workbenchSharedRenderers"/,
    );
    assert.match(transcriptSource, /<MessageRow/);
    assert.match(transcriptSource, /<ChannelUserInputCard/);
  });

  it("renders evidence and 46040 through the same channel components", () => {
    const bodySource = readFileSync(
      join(here, "../../messages/ui/MessageRowDefaultBody.tsx"),
      "utf8",
    );
    assert.match(bodySource, /parseEvidenceKind/);
    assert.match(bodySource, /<EvidenceCard/);
    const html = renderToStaticMarkup(
      React.createElement(WorkbenchCard, {
        item: {
          event: { id: "4".repeat(64), pubkey: "aa".repeat(32) },
          request: {
            request_id: "4".repeat(64),
            session_id: "s",
            turn_id: "t",
            channel_id: "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50",
            engine: "codex",
            message: "Ship it?",
            questions: [
              {
                id: "q0",
                header: "Choice",
                question: "Merge?",
                options: [{ value: "yes", label: "Yes", description: "" }],
              },
            ],
          },
        },
        currentPubkey: "bb".repeat(32),
        onSubmit: async () => {},
        onSkip: async () => {},
      }),
    );
    assert.match(html, /data-testid="channel-user-input-card-/);
    assert.match(html, /Ship it\?/);
  });
});
