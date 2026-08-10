import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const sourcePath = new URL("./useLiveHomeFeedActions.ts", import.meta.url);

test("durable home hydration uses the reusable retry state machine", async () => {
  const source = await readFile(sourcePath, "utf8");

  assert.match(source, /createHydrationRetryController/);
  assert.match(source, /hydrationRetry\.stop\(\)/);
  assert.doesNotMatch(source, /hydrationRetryTimer/);
});

test("durable live overlap is installed before history hydration starts", async () => {
  const source = await readFile(sourcePath, "utf8");
  const hydrateAfterSubscriptions = source.lastIndexOf(
    "void hydrationRetry.run();",
  );
  const startSubscriptions = source.lastIndexOf("void startSubscriptions();");

  assert.ok(hydrateAfterSubscriptions >= 0);
  assert.ok(startSubscriptions > hydrateAfterSubscriptions);
  assert.match(
    source,
    /familyDisposers\[family\] = fulfilled;[\s\S]*void hydrationRetry\.run\(\);/,
  );
});

test("durable live-buffer overflow schedules exhaustive recovery itself", async () => {
  const source = await readFile(sourcePath, "utf8");
  const overflowBranch = source.match(
    /\.length >= MAX_DURABLE_HYDRATION_BUFFER[\s\S]*?bufferedDurableEvents\.set\(event\.id, event\);/,
  )?.[0];

  assert.ok(overflowBranch, "overflow branch must remain discoverable");
  assert.match(
    overflowBranch,
    /hydrationRetry\.run\(\)/,
    "overflow must schedule a single-flight exhaustive rebuild without another event",
  );
  assert.match(
    overflowBranch,
    /family === "userInput"[\s\S]*else markAgentReceiptProjectionUnavailable/,
    "one family overflow must not make an unrelated durable family unavailable",
  );
});

test("terminal subscription failures are isolated to their projection family", async () => {
  const source = await readFile(sourcePath, "utf8");
  const permanentFailure = source.match(
    /const markFamilyPermanent =[\s\S]*?const hydrationRetry =/,
  )?.[0];

  assert.ok(
    permanentFailure,
    "family failure boundary must remain discoverable",
  );
  assert.match(permanentFailure, /family === "userInput"/);
  assert.match(
    permanentFailure,
    /isCancelled \|\| terminalFamilies\.has\(family\)/,
  );
  assert.match(permanentFailure, /familyDisposers\[candidate\]\.length > 0/);
  assert.match(permanentFailure, /hydrationRetry\.run\(\)/);
  assert.match(
    source,
    /userInputActive = false;[\s\S]*activeFamilies\.userInput = false;/,
  );
  assert.match(
    source,
    /receiptActive = false;[\s\S]*activeFamilies\.receipt = false;/,
  );
  assert.match(
    source,
    /markFamilyPermanent\(family, new Error\(status\.message\)\)/,
  );
  assert.match(source, /const familyHydrationReady:/);
  assert.doesNotMatch(source, /durableHydrationReady/);
  assert.match(
    source,
    /await handleReceiptEvent\(\s*event,[\s\S]*?\);[\s\S]*!ownsHydrationGeneration\(\)[\s\S]*familyHydrationReady\.userInput = true[\s\S]*familyHydrationReady\.receipt = true/,
    "stale async hydration cannot reopen a failed family while healthy siblings recover",
  );
});

test("approval hydration cannot terminally disable durable attention", async () => {
  const source = await readFile(sourcePath, "utf8");
  const durableHydration = source.match(
    /const hydrateDurableActions =[\s\S]*?const hydrateApprovals =/,
  )?.[0];

  assert.ok(
    durableHydration,
    "durable hydration boundary must remain discoverable",
  );
  assert.match(source, /const hydrateApprovals =/);
  assert.match(source, /const approvalHydrationRetry =/);
  assert.doesNotMatch(durableHydration, /approvalRequestEvents/);
});

test("auxiliary terminal CLOSED fails only its exact authority family", async () => {
  const source = await readFile(sourcePath, "utf8");
  const auxiliaryClosed = source.match(
    /const subscribeAuxiliary =[\s\S]*?const auxiliarySubscriptions =/,
  )?.[0];

  assert.ok(
    auxiliaryClosed,
    "auxiliary CLOSED handler must remain discoverable",
  );
  assert.match(auxiliaryClosed, /key === "reminder"/);
  assert.match(
    auxiliaryClosed,
    /failApprovalSubscriptions\(auxiliaryDisposers\)/,
  );
  assert.doesNotMatch(auxiliaryClosed, /scheduleRetry\(\)/);
  assert.match(source, /auxiliaryDisposers\[key\] = \[dispose\]/);
  assert.doesNotMatch(source, /disposeAll\(fulfilledAuxiliary\)/);
});

test("stale async validation failures cannot close a newer projection generation", async () => {
  const source = await readFile(sourcePath, "utf8");
  const userInputFailure = source.match(
    /\.catch\(\(error\) => \{[\s\S]*?Failed to validate user-input parent[\s\S]*?\}\);/,
  )?.[0];

  assert.ok(userInputFailure, "user-input validation failure path must exist");
  assert.match(
    userInputFailure,
    /durableProjectionGeneration\.userInput !==\s*eventGeneration[\s\S]*return;/,
  );
  assert.match(
    source,
    /\.catch\(\(error\) => \{[\s\S]*?!ownsGeneration\(\)[\s\S]*?Failed to validate agent receipt parent/,
  );
});
