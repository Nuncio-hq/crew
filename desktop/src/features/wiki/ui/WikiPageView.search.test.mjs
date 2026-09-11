import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import { after, before, test } from "node:test";
import { JSDOM } from "jsdom";

registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier === "@/features/wiki/ui/WikiAskBox") {
      return { shortCircuit: true, url: "crew-wiki-search-stub:ask" };
    }
    if (specifier === "@/features/wiki/ui/WikiSourceFiles") {
      return { shortCircuit: true, url: "crew-wiki-search-stub:source" };
    }
    if (specifier === "@/app/navigation/useAppNavigation") {
      return { shortCircuit: true, url: "crew-wiki-search-stub:navigation" };
    }
    return nextResolve(specifier, context);
  },
  load(url, context, nextLoad) {
    if (url === "crew-wiki-search-stub:ask") {
      return {
        format: "module",
        shortCircuit: true,
        source: "export const WikiAskBox = () => null;\n",
      };
    }
    if (url === "crew-wiki-search-stub:source") {
      return {
        format: "module",
        shortCircuit: true,
        source: "export const WikiSourceFiles = () => null;\n",
      };
    }
    if (url === "crew-wiki-search-stub:navigation") {
      return {
        format: "module",
        shortCircuit: true,
        source:
          "export const useAppNavigation = () => ({ goProject: () => {} });\n",
      };
    }
    return nextLoad(url, context);
  },
});

const OWNER = "a".repeat(64);
const COMMUNITY = "https://relay.example";
const REPO = "crew";
const SNAPSHOT = "12345678-1234-4234-9234-123456789abc";
const COORDINATE = `30617:${OWNER}:${REPO}`;
const scope = {
  scope: { owner: OWNER, community: COMMUNITY },
  workspace_generation: 1,
  identity_generation: 1,
};

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});

Object.assign(globalThis, {
  document: dom.window.document,
  HTMLElement: dom.window.HTMLElement,
  IS_REACT_ACT_ENVIRONMENT: true,
  localStorage: dom.window.localStorage,
  window: dom.window,
  ResizeObserver: class {
    disconnect() {}
    observe() {}
    unobserve() {}
  },
});

function page(id, slug, title, content) {
  return {
    id,
    pubkey: OWNER,
    kind: 30623,
    content,
    created_at: 10,
    sig: "",
    tags: [
      ["d", `${REPO}/${slug}`],
      ["a", COORDINATE],
      ["wiki-version", "1"],
      ["wiki-snapshot", SNAPSHOT],
      ["title", title],
      ["section", "overview"],
      ["commit", "c".repeat(40)],
    ],
  };
}

const firstPage = page(
  "1".repeat(64),
  "intro",
  "Introduction",
  "Stable intro body",
);
const secondPage = page(
  "2".repeat(64),
  "runtime",
  "Runtime",
  "The canonical runtime needle is here.",
);
const firstWikiPage = {
  event: firstPage,
  repoD: REPO,
  slug: "intro",
  title: "Introduction",
  section: "overview",
  commit: "c".repeat(40),
  language: "en",
  sourceFiles: [],
  content: firstPage.content,
};
const secondWikiPage = {
  event: secondPage,
  repoD: REPO,
  slug: "runtime",
  title: "Runtime",
  section: "overview",
  commit: "c".repeat(40),
  language: "en",
  sourceFiles: [],
  content: secondPage.content,
};
const graph = {
  state: "complete",
  head: {
    ...firstPage,
    content: "{}",
    tags: [
      ["d", `${REPO}/_toc`],
      ["a", COORDINATE],
      ["wiki-version", "1"],
      ["wiki-snapshot", SNAPSHOT],
      ["commit", "c".repeat(40)],
    ],
  },
  manifest: {
    ...firstPage,
    id: "3".repeat(64),
    content: JSON.stringify([
      1,
      SNAPSHOT,
      OWNER,
      REPO,
      `git:${"c".repeat(40)}`,
      null,
      [],
      [],
    ]),
    tags: [
      ["d", `${REPO}/m1-hash`],
      ["a", COORDINATE],
      ["wiki-version", "1"],
      ["wiki-snapshot", SNAPSHOT],
    ],
  },
  pages: [firstPage, secondPage],
  repoState: null,
};

let calls;

before(() => {
  calls = [];
  globalThis.__TAURI_INTERNALS__ = {
    invoke: async (command, args) => {
      calls.push({ command, args });
      if (command === "wiki_search") {
        return {
          token: scope,
          value: { events: [secondPage], truncated: false },
        };
      }
      if (command === "owner_operation_scope") return scope;
      throw new Error(`Unexpected command ${command}`);
    },
    transformCallback: () => 1,
  };
  dom.window.__TAURI_INTERNALS__ = globalThis.__TAURI_INTERNALS__;
});

after(() => dom.window.close());

const { act, fireEvent, render, screen, waitFor } = await import(
  "@testing-library/react"
);
const React = await import("react");
const { QueryClient, QueryClientProvider } = await import(
  "@tanstack/react-query"
);
const { WikiPageView } = await import("./WikiPageView.tsx");

test("the Project Wiki search input invokes scoped body search and selects a result page", async () => {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  const view = render(
    React.createElement(
      QueryClientProvider,
      { client },
      React.createElement(WikiPageView, {
        admin: false,
        askScope: "repo",
        door: "project",
        operationScope: scope,
        owner: OWNER,
        page: null,
        pages: [firstWikiPage, secondWikiPage],
        repoD: REPO,
        repoName: "Crew",
        snapshot: graph,
        toc: {
          event: graph.head,
          owner: OWNER,
          repoD: REPO,
          commit: "c".repeat(40),
          branch: "main",
          cadence: "manual",
          generatedAt: 10,
          sections: [
            {
              id: "overview",
              title: "Overview",
              pages: [
                { slug: "intro", title: "Introduction" },
                { slug: "runtime", title: "Runtime" },
              ],
            },
          ],
        },
      }),
    ),
  );
  try {
    const input = screen.getByTestId("wiki-page-search");
    await act(async () => {
      fireEvent.change(input, { target: { value: "needle" } });
    });
    await waitFor(() => {
      assert.ok(screen.getByTestId("wiki-search-results"));
      assert.ok(screen.getByTestId("wiki-search-result-runtime"));
    });
    assert.equal(
      calls.filter((call) => call.command === "wiki_search").length,
      1,
    );
    assert.equal(
      calls.find((call) => call.command === "wiki_search").args.snapshotId,
      SNAPSHOT,
    );
    await act(async () => {
      fireEvent.click(screen.getByTestId("wiki-search-result-runtime"));
    });
    assert.equal(input.value, "");
    assert.ok(screen.getByText("The canonical runtime needle is here."));
  } finally {
    view.unmount();
    client.clear();
    client.unmount();
  }
});
