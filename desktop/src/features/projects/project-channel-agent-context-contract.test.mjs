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

test("agent context names the absolute workspace but keeps cwd unchanged", () => {
  const context = projectContextForChannel(CHANNEL_ID, [project()]);
  const outgoing = appendProjectChannelAgentContext(
    "Inspect the tests.",
    context,
  );

  assert.match(outgoing, /Inspect the tests\./);
  assert.match(outgoing, /30617:/);
  assert.match(outgoing, /\/Users\/oscar\/Projects\/Nuncio Crew/);
  assert.match(outgoing, /absolute path/i);
  assert.match(outgoing, /session\/new\.cwd.+unchanged/i);
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
