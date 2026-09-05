import assert from "node:assert/strict";
import test from "node:test";

import {
  consumePendingWikiFileOpen,
  peekPendingWikiFileOpen,
  setPendingWikiFileOpen,
  wikiFileOpenMatchesProject,
  wikiFileOpenRepoId,
} from "./wikiFileOpenStore.ts";

const OWNER = "deadbeef".repeat(8);
const ROUTE_ID = `30617:${OWNER}:buzz`;
const REPO_ID = `${OWNER}:buzz`;

test("wikiFileOpenRepoId strips the 30617 coordinate prefix", () => {
  assert.equal(wikiFileOpenRepoId(ROUTE_ID), REPO_ID);
  assert.equal(wikiFileOpenRepoId(REPO_ID), REPO_ID);
});

test("wikiFileOpenMatchesProject equates entity-link and repository ids", () => {
  assert.equal(wikiFileOpenMatchesProject(ROUTE_ID, REPO_ID), true);
  assert.equal(wikiFileOpenMatchesProject(REPO_ID, ROUTE_ID), true);
  assert.equal(wikiFileOpenMatchesProject(ROUTE_ID, ROUTE_ID), true);
  assert.equal(
    wikiFileOpenMatchesProject(ROUTE_ID, `${OWNER}:relay-tools`),
    false,
  );
});

test("peek/consume match Repository.id against a 30617 pending open", () => {
  consumePendingWikiFileOpen();
  setPendingWikiFileOpen({
    projectId: ROUTE_ID,
    path: "desktop/src/features/projects/ui/ProjectDetailScreen.tsx",
    startLine: 1,
    endLine: 3,
  });
  assert.ok(peekPendingWikiFileOpen(REPO_ID));
  const consumed = consumePendingWikiFileOpen(REPO_ID);
  assert.equal(consumed?.path.endsWith("ProjectDetailScreen.tsx"), true);
  assert.equal(peekPendingWikiFileOpen(REPO_ID), null);
});

test("repository matching preserves d-tag case and embedded colons", () => {
  assert.equal(
    wikiFileOpenRepoId(`30617:${OWNER.toUpperCase()}:Crew:Docs`),
    `${OWNER}:Crew:Docs`,
  );
  assert.equal(
    wikiFileOpenMatchesProject(
      `30617:${OWNER}:Crew:Docs`,
      `${OWNER}:crew:Docs`,
    ),
    false,
  );
  assert.equal(
    wikiFileOpenMatchesProject(
      `30617:${OWNER}:Crew:Docs`,
      `${OWNER}:Crew:docs`,
    ),
    false,
  );
  assert.equal(
    wikiFileOpenMatchesProject(
      `30617:${OWNER.toUpperCase()}:Crew:Docs`,
      `${OWNER}:Crew:Docs`,
    ),
    true,
  );
});
