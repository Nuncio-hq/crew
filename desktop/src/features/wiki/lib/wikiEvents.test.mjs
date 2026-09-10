import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  defaultBranchCommit,
  wikiCanCancelRecovery,
  wikiFreshness,
  wikiRecoveryActionLabel,
  wikiRecoveryAffordance,
} from "./wikiEvents.ts";
import { debounce_due, next_cadence_due } from "./wikiCadenceJs.ts";

const COMMIT = "a".repeat(40);
const OTHER_COMMIT = "b".repeat(40);

function repoState(tags) {
  return {
    id: "state",
    pubkey: "c".repeat(64),
    kind: 30618,
    content: "",
    created_at: 1,
    tags,
    sig: "d".repeat(128),
  };
}

function toc(commit = COMMIT) {
  return { commit };
}

test("missing or unverifiable repository state leaves Wiki freshness unknown", () => {
  assert.equal(wikiFreshness(toc(), undefined), "unknown");
  assert.equal(
    wikiFreshness(toc(), repoState([["refs/heads/main", COMMIT]])),
    "unknown",
  );
  assert.equal(
    wikiFreshness(
      toc(),
      repoState([["HEAD", "ref: refs/heads/main"], ["refs/heads/main"]]),
    ),
    "unknown",
  );
});

test("freshness follows the exact advertised HEAD ref", () => {
  const state = repoState([
    ["HEAD", "ref: refs/heads/release"],
    ["refs/heads/main", OTHER_COMMIT],
    ["refs/heads/release", COMMIT],
  ]);
  assert.deepEqual(defaultBranchCommit(state), {
    branch: "release",
    commit: COMMIT,
  });
  assert.equal(wikiFreshness(toc(), state), "fresh");
});

test("a verified different HEAD commit is stale", () => {
  const state = repoState([
    ["HEAD", "ref: refs/heads/main"],
    ["refs/heads/main", OTHER_COMMIT],
  ]);
  assert.equal(wikiFreshness(toc(), state), "stale");
});

test("duplicate or malformed ref metadata is unavailable", () => {
  assert.equal(
    defaultBranchCommit(
      repoState([
        ["HEAD", "ref: refs/heads/main"],
        ["HEAD", "ref: refs/heads/release"],
        ["refs/heads/main", COMMIT],
      ]),
    ),
    null,
  );
  assert.equal(
    defaultBranchCommit(
      repoState([
        ["HEAD", "ref: refs/heads/main"],
        ["refs/heads/main", "A".repeat(40)],
      ]),
    ),
    null,
  );
});

test("time cadence stays eligible without a repository state tip", () => {
  const generatedAt = 1_000;
  assert.equal(
    next_cadence_due("daily", generatedAt, generatedAt + 86_400),
    true,
  );
  assert.equal(
    next_cadence_due("weekly", generatedAt, generatedAt + 86_400 * 7),
    true,
  );
  assert.equal(
    debounce_due(0, 0, generatedAt + 86_400 * 7),
    false,
    "on-push has no due event when the relay tip is unavailable",
  );
});

function job(overrides = {}) {
  return {
    repoKey: "owner:crew",
    operationId: "operation-1",
    operationRevision: 3,
    nativeStatus: "failed",
    reconciled: false,
    cancelRequested: false,
    reconcileOnly: false,
    status: "generating",
    done: 0,
    total: 1,
    error: null,
    costNote: null,
    ...overrides,
  };
}

test("an unresolved ambiguous cancellation offers Resume, not Retry", () => {
  const canceled = job({
    cancelRequested: true,
    reconcileOnly: true,
    headAttempted: true,
    nativeStatus: "reconciling",
  });
  assert.equal(wikiRecoveryAffordance(canceled), "resume");
  assert.equal(wikiRecoveryActionLabel("resume"), "Resume publication");
  assert.equal(
    wikiRecoveryAffordance(job({ reconcileOnly: true })),
    "resume",
    "a read-only recovery row also needs its flags revoked first",
  );
  assert.equal(wikiRecoveryAffordance(job()), "retry");
  assert.equal(wikiRecoveryActionLabel("retry"), "Retry publication");
});

test("a typed retired dependency is Regenerate-only, never Retry or Resume", () => {
  const retired = job({
    retiredDependencyId: "e".repeat(64),
    reconcileOnly: true,
    nativeStatus: "reconciling",
  });
  assert.equal(wikiRecoveryAffordance(retired), "regenerate");
  assert.equal(
    wikiRecoveryActionLabel(wikiRecoveryAffordance(retired)),
    "Regenerate from source",
  );
  assert.equal(
    wikiRecoveryAffordance({ ...retired, cancelRequested: true }),
    "regenerate",
    "the typed proof outranks an ambiguous cancellation",
  );
});

test("Cancel is suppressed for a typed retired dependency", () => {
  // Canceling there would replace the durable retirement proof with
  // CanceledBeforeHead and release the claim Regenerate needs, so native
  // refuses it; the UI must not offer the action at all.
  const retired = job({
    retiredDependencyId: "e".repeat(64),
    reconcileOnly: true,
    nativeStatus: "reconciling",
  });
  assert.equal(wikiRecoveryAffordance(retired), "regenerate");
  assert.equal(wikiCanCancelRecovery(retired), false);
  assert.equal(
    wikiCanCancelRecovery({ ...retired, reconcileOnly: false }),
    false,
    "the typed proof alone suppresses Cancel",
  );
  assert.equal(
    wikiCanCancelRecovery(job({ reconcileOnly: true })),
    true,
    "an ordinary read-only row without the typed proof stays cancelable",
  );
});

test("terminal and non-durable rows expose no publication action or Cancel", () => {
  for (const terminal of [
    job({ reconciled: true, nativeStatus: "complete" }),
    job({ reconciled: true, nativeStatus: "canceled", cancelRequested: true }),
    job({ reconciled: true, retiredDependencyId: "f".repeat(64) }),
  ]) {
    assert.equal(wikiRecoveryAffordance(terminal), "none");
    assert.equal(wikiRecoveryActionLabel("none"), null);
    assert.equal(wikiCanCancelRecovery(terminal), false);
  }
  assert.equal(wikiRecoveryAffordance(undefined), "none");
  assert.equal(wikiRecoveryAffordance(job({ operationId: undefined })), "none");
  assert.equal(wikiCanCancelRecovery(undefined), false);
  assert.equal(wikiCanCancelRecovery(job()), true);
  assert.equal(
    wikiCanCancelRecovery(job({ cancelRequested: true })),
    false,
    "an already cancelled row has nothing left to stop",
  );
});

test("both Wiki recovery surfaces use the shared affordance helper", () => {
  for (const file of [
    "../ui/WikiHeaderControls.tsx",
    "../ui/WikiRepoCard.tsx",
  ]) {
    const source = readFileSync(new URL(file, import.meta.url), "utf8");
    assert.match(source, /wikiRecoveryAffordance\(/);
    assert.match(source, /wikiRecoveryActionLabel\(/);
    assert.match(source, /wikiCanCancelRecovery\(/);
    assert.doesNotMatch(
      source,
      />\s*Retry publication\s*</,
      "action text must come from the shared helper, not a literal",
    );
  }
});

test("Resume and Retry reach native through the same explicit retry command", () => {
  const source = readFileSync(
    new URL("../hooks/useWikiPublicationRecovery.ts", import.meta.url),
    "utf8",
  );
  assert.match(
    source,
    /RecoveryAction = "retry" \| "reconcile" \| "cancel" \| "regenerate"/,
    "resume is not a separate IPC action",
  );
  assert.match(source, /`wiki_publication_\$\{input\.action\}`/);
  assert.match(
    source,
    /explicitRetry: false/,
    "the regenerate successor dispatch stays non-explicit",
  );
});

test("refresh gates missing state only for on-push cadence", () => {
  const source = readFileSync(
    new URL("../hooks/useWikiRefresh.ts", import.meta.url),
    "utf8",
  );
  assert.match(source, /toc\.cadence === "on-push" && !tip\?\.commit/);
  assert.match(source, /: nextCadenceDue\(toc, now\)/);
});
