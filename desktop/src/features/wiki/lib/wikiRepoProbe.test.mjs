import assert from "node:assert/strict";
import test from "node:test";

import {
  WIKI_EMPTY_REPO_COPY,
  WIKI_GONE_FOLDER_COPY,
  WIKI_NO_LOCAL_CHECKOUT_COPY,
  classifyWikiRepoProbe,
} from "./wikiRepoProbe.ts";

const EMPTY_COPY = "Empty repo / no default branch. Push to main, then Generate.";

test("missing local path + live GitHub default branch is not the empty-repo copy", () => {
  const probe = classifyWikiRepoProbe({
    jobError: "empty-repo",
    localWorkspacePath: null,
    localWorkspaceStatus: "unlinked",
    remoteBranch: "main",
    remoteCommit: "39c94cf005a233df12b3116c1305bb286015bc6f",
  });
  assert.notEqual(probe.kind, "empty-tree");
  assert.notEqual(probe.copy, EMPTY_COPY);
  assert.notEqual(probe.copy, WIKI_EMPTY_REPO_COPY);
  assert.equal(probe.kind, "missing-local");
  assert.equal(probe.copy, WIKI_NO_LOCAL_CHECKOUT_COPY);
  assert.equal(probe.copy, "No local checkout found.");
  assert.equal(probe.showGenerate, true);
});

test("unbound path + stale empty-repo job does not blame GitHub when remote has main", () => {
  const probe = classifyWikiRepoProbe({
    jobError: "empty-repo",
    localWorkspacePath: undefined,
    localWorkspaceStatus: "unlinked",
    remoteBranch: "main",
    remoteCommit: "abc123",
  });
  assert.equal(probe.copy.includes("no default branch"), false);
  assert.equal(probe.copy.includes("Push to main"), false);
  assert.equal(probe.showGenerate, true);
});

test("gone bound folder + remote main uses the existing pick-folder copy", () => {
  const probe = classifyWikiRepoProbe({
    jobError: "empty-repo",
    localWorkspacePath: "/Users/oscar/Desktop/Oscar/LilGroup/Nuncio/crew",
    localWorkspaceStatus: "linked",
    remoteBranch: "main",
    remoteCommit: "39c94cf005a233df12b3116c1305bb286015bc6f",
  });
  assert.notEqual(probe.kind, "empty-tree");
  assert.notEqual(probe.copy, EMPTY_COPY);
  assert.equal(probe.kind, "missing-local-gone");
  assert.equal(probe.copy, WIKI_GONE_FOLDER_COPY);
  assert.equal(
    probe.copy,
    "The Project folder is gone. Pick a workspace again.",
  );
  assert.equal(probe.showGenerate, true);
});

test("worker missing-local-path with no folder uses No local checkout found", () => {
  const probe = classifyWikiRepoProbe({
    jobError: "missing-local-path",
    localWorkspacePath: null,
    localWorkspaceStatus: "unlinked",
    remoteBranch: "main",
    remoteCommit: "abc123",
  });
  assert.equal(probe.kind, "missing-local");
  assert.equal(probe.copy, "No local checkout found.");
  assert.equal(probe.showGenerate, true);
});

test("actually empty local git tree still uses the empty-repo copy", () => {
  const probe = classifyWikiRepoProbe({
    jobError: "empty-repo",
    localWorkspacePath: "/tmp/empty-git",
    localWorkspaceStatus: "linked",
    remoteBranch: null,
    remoteCommit: null,
  });
  assert.equal(probe.kind, "empty-tree");
  assert.equal(probe.copy, WIKI_EMPTY_REPO_COPY);
  assert.equal(probe.copy, EMPTY_COPY);
  assert.equal(probe.showGenerate, false);
});

test("idle job with a bound path is not an empty-repo notice", () => {
  const probe = classifyWikiRepoProbe({
    jobError: null,
    localWorkspacePath: "/workspace",
    localWorkspaceStatus: "linked",
    remoteBranch: "main",
    remoteCommit: "abc123",
  });
  assert.equal(probe.kind, "ok");
  assert.equal(probe.copy, null);
  assert.equal(probe.showGenerate, true);
});
