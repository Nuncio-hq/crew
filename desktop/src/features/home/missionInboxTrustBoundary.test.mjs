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
const missionSource = readFileSync(
  new URL("./lib/missionInbox.ts", import.meta.url),
  "utf8",
);
const homeViewSource = readFileSync(
  new URL("./ui/HomeView.tsx", import.meta.url),
  "utf8",
);
const verifiedMissionSource = readFileSync(
  new URL("./useVerifiedMissionSelection.ts", import.meta.url),
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
  assert.doesNotMatch(
    missionSource,
    /getThreadReference\(row\.inboxItem\.item\.tags\)/,
  );
  assert.match(missionSource, /event\.id !== messageId/);
  assert.match(missionSource, /tag\[0\] === "h"/);
  assert.doesNotMatch(homeViewSource, /onOpenContext\(\s*row\.channelId/);
  assert.match(
    verifiedMissionSource,
    /openMissionContext\(\s*target\.channelId/,
  );
  assert.match(homeViewSource, /activeVerifiedMissionTarget\?\.channelId/);
});

test("stale verified row lookups cannot supersede a newer selection", () => {
  assert.match(verifiedMissionSource, /selectionGenerationRef/);
  assert.match(
    verifiedMissionSource,
    /await getMissionInboxEventTarget\(row\);[\s\S]*selectionGenerationRef\.current !== generation[\s\S]*openMissionContext\(/,
  );
  assert.match(
    verifiedMissionSource,
    /await getMissionInboxEventTarget\(row\);[\s\S]*selectionGenerationRef\.current !== generation[\s\S]*setVerifiedTarget\(target\)/,
  );
  assert.match(
    homeViewSource,
    /handleOpenProfilePanel[\s\S]*clearVerifiedTarget\(\)/,
  );
  assert.match(
    homeViewSource,
    /handleFilterChange[\s\S]*clearVerifiedTarget\(\)/,
  );
  assert.match(
    homeViewSource,
    /onOpenDirect=\{\(item\) => \{[\s\S]*clearVerifiedTarget\(\)/,
  );
  assert.match(
    homeViewSource,
    /onRemindLater=\{\(item\) => \{[\s\S]*clearVerifiedTarget\(\)/,
  );
  assert.match(
    homeViewSource,
    /onUnreadOnlyChange=\{\(nextUnreadOnly\) => \{[\s\S]*clearVerifiedTarget\(\)/,
  );
});

test("unsigned feed rows cannot mutate durable approval authority", () => {
  for (const source of [hooksSource, sectionsSource, needsYouSource]) {
    assert.doesNotMatch(source, /ingestApprovalRequestFeedItem/);
    assert.doesNotMatch(source, /reconcileNeedsYouFromFeed/);
  }
});
