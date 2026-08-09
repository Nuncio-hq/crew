import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const inboxSource = readFileSync(
  new URL("./lib/inbox.ts", import.meta.url),
  "utf8",
);
const contextSource = readFileSync(
  new URL("./useInboxThreadContext.ts", import.meta.url),
  "utf8",
);
const hooksSource = readFileSync(
  new URL("./hooks.ts", import.meta.url),
  "utf8",
);
const sectionsSource = readFileSync(
  new URL("./useMissionInboxSections.ts", import.meta.url),
  "utf8",
);
const needsYouSource = readFileSync(
  new URL("../agents/needsYouStore.ts", import.meta.url),
  "utf8",
);

test("feed summaries cannot be reconstructed as trusted relay events", () => {
  assert.doesNotMatch(inboxSource, /relayEventFromFeedItem/);
  assert.doesNotMatch(inboxSource, /sig:\s*["']{2}/);
  assert.match(contextSource, /isVerifiedRelayEvent\(event\)/);
  assert.match(
    contextSource,
    /isVerifiedRelayEvent\(event\)[\s\S]*isInboxThreadContextEvent\(event, selectedVerifiedSelection\)/,
  );
  assert.match(contextSource, /event\.id !== expectedEventId/);
  assert.doesNotMatch(contextSource, /getThreadReference\(item\.tags\)/);
  assert.doesNotMatch(contextSource, /item\?\.channelId/);
});

test("unsigned feed rows cannot mutate durable approval authority", () => {
  for (const source of [hooksSource, sectionsSource, needsYouSource]) {
    assert.doesNotMatch(source, /ingestApprovalRequestFeedItem/);
    assert.doesNotMatch(source, /reconcileNeedsYouFromFeed/);
  }
});
