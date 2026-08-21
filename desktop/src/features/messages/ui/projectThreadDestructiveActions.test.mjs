import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const workspaceUrl = new URL(
  "./ProjectThreadWorkspaceDetails.tsx",
  import.meta.url,
);
const githubUrl = new URL("./ProjectThreadGitHubDetails.tsx", import.meta.url);
const githubRowUrl = new URL("./ProjectThreadGitHubRow.tsx", import.meta.url);
const statusUrl = new URL(
  "../lib/projectThreadGitHubStatus.ts",
  import.meta.url,
);

test("workspace destructive actions use an in-app confirmation with a cancel path", async () => {
  const source = await readFile(workspaceUrl, "utf8");

  assert.doesNotMatch(source, /window\.confirm/);
  assert.match(source, /AlertDialogCancel/);
  assert.match(source, /data-testid="project-thread-workspace-confirm-action"/);
  assert.match(source, /setPendingAction\(null\)/);
  assert.doesNotMatch(
    source,
    /Ignored and other local files inside the checkout are removed/,
  );
  assert.match(source, /Ignored local files block eviction/);
  assert.match(source, /hasIgnoredLocalState === true/);
});

test("close PR uses an in-app confirmation and does not run from cancel", async () => {
  const source = await readFile(githubUrl, "utf8");

  assert.doesNotMatch(source, /window\.confirm/);
  assert.match(source, /AlertDialogCancel/);
  assert.match(source, /data-testid="project-thread-close-pr-confirm-action"/);
  assert.match(source, /setConfirmOpen\(false\);\s*void close\(\)/s);
});

test("workspace lifecycle buttons fit their grid cells", async () => {
  const source = await readFile(workspaceUrl, "utf8");

  assert.match(source, /grid min-w-0 grid-cols-1/);
  assert.match(source, /whitespace-normal px-2 py-2 leading-tight/);
  assert.match(
    source,
    /<span className="text-center">Free local space<\/span>/,
  );
});

test("GitHub status colors cover PR and CI states", async () => {
  const statusSource = await readFile(statusUrl, "utf8");
  const rowSource = await readFile(githubRowUrl, "utf8");

  assert.match(statusSource, /state === "MERGED"/);
  assert.match(statusSource, /state === "CLOSED"/);
  assert.match(statusSource, /pullRequest\.isDraft/);
  assert.match(statusSource, /return \{ label: "Open"/);
  assert.match(statusSource, /return \{ label: "Checks failing"/);
  assert.match(statusSource, /return \{ label: "Checks running"/);
  assert.match(statusSource, /return \{ label: "Checks passing"/);
  assert.match(rowSource, /statusClassName={projectThreadStatusClassName/);
});
