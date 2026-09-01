import assert from "node:assert/strict";
import test from "node:test";

import { buildAgentAvatarUrlsByName } from "./buildAgentAvatarUrlsByName.ts";

const AGENT = "aa".repeat(32);

test("buildAgentAvatarUrlsByName uses the mention-candidate runtime face", () => {
  const values = buildAgentAvatarUrlsByName({
    mentionCandidates: [
      {
        kind: "identity",
        pubkey: AGENT,
        displayName: "Hermes Default",
        avatarUrl: "/harness-logos/hermes.png",
        isMember: true,
        isAgent: true,
      },
    ],
    mentionMap: new Map([["Hermes Default", AGENT]]),
    profiles: {},
    selectedAgentMentionNames: ["Hermes Default"],
  });

  assert.equal(values["hermes default"], "/harness-logos/hermes.png");
});

test("buildAgentAvatarUrlsByName prefers the live profile over the candidate", () => {
  const values = buildAgentAvatarUrlsByName({
    mentionCandidates: [
      {
        kind: "identity",
        pubkey: AGENT,
        displayName: "Claude",
        avatarUrl: "/harness-logos/claude.png",
        isMember: true,
        isAgent: true,
      },
    ],
    mentionMap: new Map([["Claude", AGENT]]),
    profiles: {
      [AGENT]: {
        displayName: "Claude",
        avatarUrl: "https://relay.example/kind0.png",
        nip05Handle: null,
        ownerPubkey: null,
        isAgent: true,
      },
    },
    selectedAgentMentionNames: ["Claude"],
  });

  assert.equal(values.claude, "https://relay.example/kind0.png");
});

test("buildAgentAvatarUrlsByName uses a persona runtime face when no pubkey profile exists", () => {
  const values = buildAgentAvatarUrlsByName({
    mentionCandidates: [
      {
        kind: "persona",
        personaId: "persona-goose",
        displayName: "Goose",
        avatarUrl: "/harness-logos/goose.svg",
        isMember: false,
        isAgent: true,
      },
    ],
    mentionMap: new Map(),
    profiles: {},
    selectedAgentMentionNames: ["Goose"],
  });

  assert.equal(values.goose, "/harness-logos/goose.svg");
});
