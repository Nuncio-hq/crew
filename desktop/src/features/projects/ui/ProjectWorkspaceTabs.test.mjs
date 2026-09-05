import assert from "node:assert/strict";
import test from "node:test";

import { getPullRequestFilesBadgeCount } from "./ProjectWorkspaceTabs.tsx";

function snapshotWithFiles(count) {
  return {
    files: Array.from({ length: count }, (_, index) => ({
      path: `file-${index}.ts`,
    })),
  };
}

test("uses zero for the Files changed badge when no diff is available", () => {
  const snapshot = snapshotWithFiles(250);

  assert.equal(
    getPullRequestFilesBadgeCount(null, false),
    0,
    "the repository snapshot file count must not become a diff badge",
  );
  assert.equal(snapshot.files.length, 250);
});

test("uses the diff file count for the Files changed badge", () => {
  const diff = { files: snapshotWithFiles(3).files };

  assert.equal(getPullRequestFilesBadgeCount(diff, false), 3);
});

test("hides the Files changed badge count while the diff is loading", () => {
  const diff = { files: snapshotWithFiles(3).files };

  assert.equal(getPullRequestFilesBadgeCount(diff, true), null);
});

test("WorkspaceTabs derives the badge only through getPullRequestFilesBadgeCount", async () => {
  const { readFile } = await import("node:fs/promises");
  const source = await readFile(
    new URL("./ProjectWorkspaceTabs.tsx", import.meta.url),
    "utf8",
  );

  assert.match(
    source,
    /filesCount=\{\s*getPullRequestFilesBadgeCount\(/,
    "the Files changed badge must come from getPullRequestFilesBadgeCount",
  );
  assert.doesNotMatch(
    source,
    /filesCount=\{[^}]*\?\?\s*files\.length/,
    "the snapshot file listing must never be a badge fallback again",
  );
});
