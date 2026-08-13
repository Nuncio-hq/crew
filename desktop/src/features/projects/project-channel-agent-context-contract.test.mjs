import assert from "node:assert/strict";
import { test } from "node:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import ReactMarkdown from "react-markdown";

import {
  appendProjectChannelAgentContext,
  projectContextForChannel,
} from "./lib/project-channel-agent-context.ts";

const CHANNEL_ID = "018f30b4-57c0-7f10-a3f8-9f7d8e6c5b4a";
const LOCAL_PATH = "/Users/oscar/Projects/Nuncio Crew";

function project(overrides = {}) {
  return {
    id: `${"a".repeat(64)}:crew`,
    repoAddress: `30617:${"a".repeat(64)}:crew`,
    projectChannelId: CHANNEL_ID,
    localWorkspace: { status: "linked", path: LOCAL_PATH },
    ...overrides,
  };
}

test("resolves workspace context only for the matching Project channel", () => {
  assert.deepEqual(projectContextForChannel(CHANNEL_ID, [project()]), {
    status: "ready",
    repoAddress: `30617:${"a".repeat(64)}:crew`,
    localPath: LOCAL_PATH,
    workspaceMode: "git",
  });
  assert.deepEqual(projectContextForChannel("another-channel", [project()]), {
    status: "none",
  });
});

test("an unlinked Project sends no workspace context", () => {
  assert.deepEqual(
    projectContextForChannel(CHANNEL_ID, [
      project({ localWorkspace: { status: "unlinked" } }),
    ]),
    { status: "none" },
  );
});

test("invalid workspace metadata fails closed", () => {
  const context = projectContextForChannel(CHANNEL_ID, [
    project({
      localWorkspace: {
        status: "invalid",
        reason: "invalid-local-path",
      },
    }),
  ]);

  assert.deepEqual(context, {
    status: "invalid",
    reason: "invalid-local-workspace",
  });
  assert.throws(
    () => appendProjectChannelAgentContext("@codex inspect.", context),
    /invalid local workspace/i,
  );
});

test("duplicate Project bindings fail closed instead of choosing one", () => {
  assert.deepEqual(
    projectContextForChannel(CHANNEL_ID, [
      project(),
      project({
        id: `${"b".repeat(64)}:other`,
        repoAddress: `30617:${"b".repeat(64)}:other`,
      }),
    ]),
    { status: "invalid", reason: "duplicate-project-channel-binding" },
  );
});

test("each send resolves the current path rather than caching an older link", () => {
  const before = projectContextForChannel(CHANNEL_ID, [project()]);
  const after = projectContextForChannel(CHANNEL_ID, [
    project({
      localWorkspace: {
        status: "linked",
        path: "/Users/oscar/Projects/Nuncio Crew v2",
      },
    }),
  ]);

  assert.equal(before.localPath, LOCAL_PATH);
  assert.equal(after.localPath, "/Users/oscar/Projects/Nuncio Crew v2");
});

test("agent context encodes the source workspace for per-thread provisioning", () => {
  const context = projectContextForChannel(CHANNEL_ID, [project()]);
  const outgoing = appendProjectChannelAgentContext(
    "Inspect the tests.",
    context,
  );

  assert.match(outgoing, /Inspect the tests\./);
  assert.match(outgoing, /30617:/);
  assert.match(outgoing, /\/Users\/oscar\/Projects\/Nuncio Crew/);
  assert.match(outgoing, /path=%2FUsers%2Foscar%2FProjects%2FNuncio%20Crew/);
  assert.match(outgoing, /isolated worktree per thread/i);
});

test("Cowork context uses mode=folder and never mentions git worktrees", () => {
  const context = projectContextForChannel(CHANNEL_ID, [
    project({ workspaceMode: "folder" }),
  ]);
  const outgoing = appendProjectChannelAgentContext(
    "Draft the proposal.",
    context,
  );
  assert.match(outgoing, /mode=folder/);
  assert.match(outgoing, /Version history is kept automatically/);
  assert.doesNotMatch(outgoing, /worktree/i);
  assert.doesNotMatch(outgoing, /\bgit\b/i);
});

test("machine context is invisible in rendered CommonMark", () => {
  const context = projectContextForChannel(CHANNEL_ID, [project()]);
  const outgoing = appendProjectChannelAgentContext(
    "@codex inspect the tests.",
    context,
  );
  const rendered = renderToStaticMarkup(
    React.createElement(ReactMarkdown, null, outgoing),
  );

  assert.equal(rendered, "<p>@codex inspect the tests.</p>");
});

test("machine context stays hidden after an unclosed code fence", () => {
  const context = projectContextForChannel(CHANNEL_ID, [project()]);
  const outgoing = appendProjectChannelAgentContext(
    "```text\ninspect this draft",
    context,
  );
  const rendered = renderToStaticMarkup(
    React.createElement(ReactMarkdown, null, outgoing),
  );

  assert.doesNotMatch(rendered, /buzz-project-context|30617:|Nuncio Crew/);
  assert.match(rendered, /inspect this draft/);
});

test("a manager reference-label collision cannot expose machine context", () => {
  const context = projectContextForChannel(CHANNEL_ID, [project()]);
  const outgoing = appendProjectChannelAgentContext(
    "Keep [buzz-project-context] as manager text.",
    context,
  );
  const rendered = renderToStaticMarkup(
    React.createElement(ReactMarkdown, null, outgoing),
  );

  assert.equal(rendered, "<p>Keep [buzz-project-context] as manager text.</p>");
  assert.doesNotMatch(rendered, /30617:|Nuncio Crew/);
});

test("CommonMark-normalized label variants cannot expose machine context", () => {
  const context = projectContextForChannel(CHANNEL_ID, [project()]);
  const outgoing = appendProjectChannelAgentContext(
    "Keep [buzz-project-context ] and [x][buzz-project-context ] literal.",
    context,
  );
  const rendered = renderToStaticMarkup(
    React.createElement(ReactMarkdown, null, outgoing),
  );

  assert.equal(
    rendered,
    "<p>Keep [buzz-project-context ] and [x][buzz-project-context ] literal.</p>",
  );
  assert.doesNotMatch(rendered, /href=|30617:|Nuncio Crew/);
});

test("ordinary channel messages are unchanged", () => {
  assert.equal(
    appendProjectChannelAgentContext("Hello team.", { status: "none" }),
    "Hello team.",
  );
});

test("absent binding params keep today's workspace URL", () => {
  const context = projectContextForChannel(CHANNEL_ID, [project()]);
  const outgoing = appendProjectChannelAgentContext("@codex inspect.", context);
  assert.match(
    outgoing,
    /buzz:\/\/project-workspace\?repo=30617%3A[a-f0-9]+%3Acrew&path=%2FUsers%2Foscar%2FProjects%2FNuncio%20Crew>/,
  );
  assert.doesNotMatch(outgoing, /[?&]ws=/);
  assert.doesNotMatch(outgoing, /[?&]base=/);
});

test("non-default bindings add ws or base query params", () => {
  const context = projectContextForChannel(CHANNEL_ID, [project()]);
  const main = appendProjectChannelAgentContext(
    "@codex inspect.",
    context,
    { mode: "main" },
    "main",
  );
  assert.match(main, /&ws=main>/);
  const branch = appendProjectChannelAgentContext(
    "@codex inspect.",
    context,
    { mode: "branch", name: "feature/x" },
    "main",
  );
  assert.match(branch, /&ws=branch%3Afeature%2Fx>/);
  const namedBase = appendProjectChannelAgentContext(
    "@codex inspect.",
    context,
    { mode: "new", base: "release" },
    "main",
  );
  assert.match(namedBase, /&base=release>/);
});
