import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";

async function source(relativePath) {
  return readFile(new URL(relativePath, import.meta.url), "utf8");
}

test("the send path appends visible-page context after workspace context", async () => {
  const hook = await source("../messages/ui/crewSendContext.ts");
  const resolverIndex = hook.indexOf(
    "await resolveCurrentProjectChannelAgentMessage",
  );
  assert.ok(resolverIndex >= 0);
  const viewIndex = hook.indexOf("appendCrewViewAgentContext(", resolverIndex);
  assert.ok(
    viewIndex > resolverIndex,
    "visible-page context must be appended after the workspace context resolve",
  );
  // Hidden context is agent-only: both steps sit inside the explicit-agent guard.
  const guardIndex = hook.lastIndexOf(
    "if (explicitAgentPubkeys.length > 0) {",
    viewIndex,
  );
  assert.ok(guardIndex >= 0 && guardIndex < resolverIndex);
  // A resolve failure must throw before any content/recipient result exists.
  const throwIndex = hook.indexOf("throw new Error(message", resolverIndex);
  assert.ok(throwIndex > resolverIndex && throwIndex < viewIndex);
  const flow = await source("../messages/ui/useMentionSendFlow.ts");
  assert.ok(flow.includes("useCrewSendContext()"));
});

test("Crew mounts the visible-page provider in its own thread chrome", async () => {
  const channelPane = await source("../channels/ui/ChannelPane.tsx");
  const threadPanel = await source("../messages/ui/MessageThreadPanel.tsx");
  assert.ok(threadPanel.includes("<ThreadComposerViewContext"));
  const wrapper = await source("../messages/ui/ThreadComposerViewContext.tsx");
  assert.ok(
    wrapper.includes("<ComposerViewContextProvider"),
    "the thread wrapper must mount the Crew-owned visible-page provider",
  );
  // The channel dock shows nothing beyond the channel itself, so its sends stay
  // byte-identical to what the sender typed.
  assert.equal(channelPane.includes("ComposerViewContextProvider"), false);
  // Guardrail #278: no upstream Projects chrome comes back with this context.
  for (const forbidden of ["ProjectAgentChatPanel", "ProjectsOverview"]) {
    assert.equal(channelPane.includes(forbidden), false);
    assert.equal(threadPanel.includes(forbidden), false);
  }
});
