import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";

async function source(relativePath) {
  return readFile(new URL(relativePath, import.meta.url), "utf8");
}

test("the send path appends visible-page context after workspace context", async () => {
  const hook = await source("../messages/ui/useMentionSendComplete.ts");
  const resolverIndex = hook.indexOf(
    "await resolveCurrentProjectChannelAgentMessage",
  );
  assert.ok(resolverIndex >= 0);
  const viewIndex = hook.indexOf("appendCrewViewAgentContext(", resolverIndex);
  assert.ok(
    viewIndex > resolverIndex,
    "visible-page context must be appended after the workspace context resolve",
  );
  const sendIndex = hook.indexOf("await send(", viewIndex);
  assert.ok(sendIndex > viewIndex, "context must be applied before send");

  // Hidden context is agent-only: it must live inside the explicit-agent guard.
  const guardIndex = hook.lastIndexOf(
    "if (effectiveExplicitAgentPubkeys.length > 0) {",
    viewIndex,
  );
  assert.ok(guardIndex >= 0 && guardIndex < viewIndex);
  assert.ok(hook.includes("useComposerViewContext()"));
});

test("Crew mounts the visible-page provider in its own channel and thread chrome", async () => {
  const channelPane = await source("../channels/ui/ChannelPane.tsx");
  const threadPanel = await source("../messages/ui/MessageThreadPanel.tsx");
  assert.ok(channelPane.includes("<ChannelComposerContextProviders"));
  assert.ok(threadPanel.includes("<ThreadComposerViewContext"));
  for (const wrapper of [
    "../channels/ui/ChannelComposerContextProviders.tsx",
    "../messages/ui/ThreadComposerViewContext.tsx",
  ]) {
    const text = await source(wrapper);
    assert.ok(
      text.includes("<ComposerViewContextProvider"),
      `${wrapper} must mount the Crew-owned visible-page provider`,
    );
  }
  // Guardrail #278: no upstream Projects chrome comes back with this context.
  for (const forbidden of ["ProjectAgentChatPanel", "ProjectsOverview"]) {
    assert.equal(channelPane.includes(forbidden), false);
    assert.equal(threadPanel.includes(forbidden), false);
  }
});
