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

test("Wiki citations update repeated file ranges without remounting or consuming another repository", async () => {
  const React = await import("react");
  const { JSDOM } = await import("jsdom");
  const { QueryClient, QueryClientProvider } = await import(
    "@tanstack/react-query"
  );
  const { RepositoryFilesPanel } = await import("./ProjectRepositoryPanel.tsx");
  const {
    setPendingWikiFileOpen,
    peekPendingWikiFileOpen,
    consumePendingWikiFileOpen,
  } = await import("../../wiki/lib/wikiFileOpenStore.ts");
  const dom = new JSDOM("<!doctype html><html><body></body></html>", {
    url: "http://localhost",
  });
  const originals = new Map(
    [
      "window",
      "document",
      "navigator",
      "HTMLElement",
      "IS_REACT_ACT_ENVIRONMENT",
    ].map((key) => [key, Object.getOwnPropertyDescriptor(globalThis, key)]),
  );
  for (const [key, value] of Object.entries({
    window: dom.window,
    document: dom.window.document,
    navigator: dom.window.navigator,
    HTMLElement: dom.window.HTMLElement,
    IS_REACT_ACT_ENVIRONMENT: true,
  })) {
    Object.defineProperty(globalThis, key, {
      configurable: true,
      writable: true,
      value,
    });
  }
  dom.window.HTMLElement.prototype.scrollIntoView = () => {};
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: Infinity } },
  });
  const { render, cleanup } = await import("@testing-library/react");
  const owner = "ab".repeat(32);
  const projectId = `${owner}:crew`;
  const otherProjectId = `${owner}:other`;
  const files = [
    {
      path: "notes.txt",
      size: 30,
      previewContent: "one\ntwo\nthree\nfour\nfive",
      lastChangedAt: 0,
    },
  ];
  const pending = (repository, startLine, endLine, path = "notes.txt") => ({
    projectId: `30617:${repository}`,
    path,
    startLine,
    endLine,
  });
  const ui = (requestKey, repository = projectId, shownFiles = files) =>
    React.createElement(
      QueryClientProvider,
      { client },
      React.createElement(RepositoryFilesPanel, {
        files: shownFiles,
        projectId: repository,
        fileOpenRequestKey: requestKey,
        initialPath: "notes.txt",
        snapshot: null,
        isLoading: false,
        error: null,
      }),
    );
  try {
    consumePendingWikiFileOpen();
    setPendingWikiFileOpen(pending(projectId, 1, 2));
    const view = render(ui("first"));
    const highlighted = () =>
      view
        .getAllByTestId("wiki-file-highlight")
        .map((element) => element.getAttribute("data-line"));
    assert.deepEqual(highlighted(), ["1", "2"]);
    const firstPanel = view.getByTestId("wiki-file-panel");
    assert.equal(peekPendingWikiFileOpen(), null);

    setPendingWikiFileOpen(pending(projectId, 4, 5));
    view.rerender(ui("repeat"));
    assert.deepEqual(highlighted(), ["4", "5"]);
    assert.equal(
      view.getByTestId("wiki-file-panel"),
      firstPanel,
      "range changes must not remount the file panel",
    );

    const caseDistinctProjectId = `${owner}:Crew`;
    setPendingWikiFileOpen(pending(caseDistinctProjectId, 2, 3));
    view.rerender(ui("case-distinct-repo-pending"));
    assert.deepEqual(highlighted(), ["4", "5"]);
    assert.ok(
      peekPendingWikiFileOpen(caseDistinctProjectId),
      "case-distinct repository identifiers must not consume each other's citation",
    );

    setPendingWikiFileOpen(pending(otherProjectId, 2, 3));
    view.rerender(ui("other-repo-pending"));
    assert.deepEqual(highlighted(), ["4", "5"]);
    assert.ok(
      peekPendingWikiFileOpen(otherProjectId),
      "current repository must not consume another repository's citation",
    );
    view.rerender(ui("other-repo-arrived", otherProjectId));
    assert.deepEqual(highlighted(), ["2", "3"]);
    assert.equal(peekPendingWikiFileOpen(), null);

    setPendingWikiFileOpen(pending(otherProjectId, 1, 1, "later.txt"));
    view.rerender(ui("file-pending", otherProjectId));
    assert.ok(
      peekPendingWikiFileOpen(otherProjectId),
      "a snapshot without the file must not discard the citation",
    );
    view.rerender(
      ui("file-pending", otherProjectId, [
        ...files,
        { ...files[0], path: "later.txt" },
      ]),
    );
    assert.deepEqual(highlighted(), ["1"]);
    assert.equal(peekPendingWikiFileOpen(), null);
  } finally {
    cleanup();
    client.clear();
    consumePendingWikiFileOpen();
    dom.window.close();
    for (const [key, descriptor] of originals) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else delete globalThis[key];
    }
  }
});
