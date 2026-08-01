import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const workspaceUrl = new URL(
  "./ProjectThreadWorkspaceDetails.tsx",
  import.meta.url,
);
const githubUrl = new URL("./ProjectThreadGitHubDetails.tsx", import.meta.url);

test("workspace destructive actions use an in-app confirmation with a cancel path", async () => {
  const source = await readFile(workspaceUrl, "utf8");

  assert.doesNotMatch(source, /window\.confirm/);
  assert.match(source, /AlertDialogCancel/);
  assert.match(source, /data-testid="project-thread-workspace-confirm-action"/);
  assert.match(source, /setPendingAction\(null\)/);
});

test("close PR uses an in-app confirmation and does not run from cancel", async () => {
  const source = await readFile(githubUrl, "utf8");

  assert.doesNotMatch(source, /window\.confirm/);
  assert.match(source, /AlertDialogCancel/);
  assert.match(source, /data-testid="project-thread-close-pr-confirm-action"/);
  assert.match(source, /setConfirmOpen\(false\);\s*void close\(\)/s);
});
