import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import { after, beforeEach, test } from "node:test";
import { JSDOM } from "jsdom";

registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier === "@/features/wiki/ui/WikiAskBox") {
      return { shortCircuit: true, url: "crew-wiki-navigation-stub:ask" };
    }
    if (specifier === "@/features/wiki/ui/WikiSourceFiles") {
      return { shortCircuit: true, url: "crew-wiki-navigation-stub:source" };
    }
    if (specifier === "@/app/navigation/useAppNavigation") {
      return {
        shortCircuit: true,
        url: "crew-wiki-navigation-stub:navigation",
      };
    }
    return nextResolve(specifier, context);
  },
  load(url, context, nextLoad) {
    if (url === "crew-wiki-navigation-stub:ask") {
      return {
        format: "module",
        shortCircuit: true,
        source: "export const WikiAskBox = () => null;\n",
      };
    }
    if (url === "crew-wiki-navigation-stub:source") {
      return {
        format: "module",
        shortCircuit: true,
        source: `
          export const WikiSourceFiles = ({
            operationScope,
            owner,
            pageEvent,
            repoD,
          }) =>
            globalThis.React.createElement(
              "output",
              {
                "data-community": operationScope?.scope?.community ?? "",
                "data-coordinate":
                  pageEvent?.tags?.find((tag) => tag[0] === "a")?.[1] ?? "",
                "data-owner": owner ?? "",
                "data-repo-d": repoD ?? "",
                "data-testid": "wiki-source-identity",
              },
              "source",
            );
        `,
      };
    }
    if (url === "crew-wiki-navigation-stub:navigation") {
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
const PROJECT_ID = "project-1";
const REPO_D = "crew";
const COORDINATE = `30617:${OWNER}:${REPO_D}`;
const SNAPSHOT = "12345678-1234-4234-9234-123456789abc";
const scope = {
  scope: { owner: OWNER, community: COMMUNITY },
  workspace_generation: 1,
  identity_generation: 1,
};
const nextScope = {
  ...scope,
  scope: { ...scope.scope, community: "https://other.example" },
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

const { act, cleanup, fireEvent, render, screen, waitFor } = await import(
  "@testing-library/react"
);
const React = await import("react");
globalThis.React = React;
const { QueryClient, QueryClientProvider } = await import(
  "@tanstack/react-query"
);
const { readWikiNavigationState, writeWikiNavigationState } = await import(
  "../lib/wikiNavigationState.ts"
);
const { parseWikiPage } = await import("../lib/wikiEvents.ts");
const { WikiPageView } = await import("./WikiPageView.tsx");

function page(id, slug, title, content) {
  return {
    event: {
      id,
      pubkey: OWNER,
      kind: 30623,
      content,
      created_at: 10,
      sig: "",
      tags: [
        ["d", `${REPO_D}/${slug}`],
        ["a", COORDINATE],
        ["wiki-version", "1"],
        ["wiki-snapshot", SNAPSHOT],
        ["title", title],
      ],
    },
    repoD: REPO_D,
    slug,
    title,
    section: "overview",
    commit: "c".repeat(40),
    language: "en",
    sourceFiles: [],
    content,
  };
}

function v1Page(id, addressSlug, logicalSlug, title, content) {
  const event = {
    id,
    pubkey: OWNER,
    kind: 30623,
    content,
    created_at: 10,
    sig: "",
    tags: [
      ["d", `${REPO_D}/${addressSlug}`],
      ["a", COORDINATE],
      ["wiki-version", "1"],
      ["wiki-snapshot", SNAPSHOT],
      ["wiki-slug", logicalSlug],
      ["title", title],
      ["section", "overview"],
      ["language", "en"],
    ],
  };
  const parsed = parseWikiPage(event);
  assert.ok(parsed, "the v1 event must cross the production parser seam");
  return parsed;
}

function tocForPages(pages) {
  return {
    ...toc,
    event: pages[0]?.event ?? toc.event,
    sections: [
      {
        ...toc.sections[0],
        pages: pages.map((item) => ({
          slug: item.slug,
          title: item.title,
        })),
      },
    ],
  };
}

const intro = page("1".repeat(64), "intro", "Introduction", "Intro body");
const runtime = page("2".repeat(64), "runtime", "Runtime", "Runtime body");
const verifiedSnapshot = {
  state: "complete",
  head: {
    ...intro.event,
    id: "3".repeat(64),
    tags: [
      ["d", `${REPO_D}/_toc`],
      ["a", COORDINATE],
      ["wiki-version", "1"],
      ["wiki-snapshot", SNAPSHOT],
    ],
  },
  manifest: {
    ...intro.event,
    id: "4".repeat(64),
    content: JSON.stringify([
      1,
      SNAPSHOT,
      OWNER,
      REPO_D,
      `git:${"c".repeat(40)}`,
      null,
      [],
      [],
      [],
    ]),
    tags: [
      ["d", `${REPO_D}/manifest`],
      ["a", COORDINATE],
      ["wiki-version", "1"],
      ["wiki-snapshot", SNAPSHOT],
    ],
  },
  pages: [intro.event, runtime.event],
  repoState: null,
};
const toc = {
  event: intro.event,
  owner: OWNER,
  repoD: REPO_D,
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
};

function identity(overrides = {}) {
  return {
    community: COMMUNITY,
    viewer: OWNER,
    projectId: PROJECT_ID,
    repositoryCoordinate: COORDINATE,
    surface: "project",
    ...overrides,
  };
}

function viewProps(overrides = {}) {
  return {
    admin: false,
    askScope: "repo",
    isCompany: false,
    door: "project",
    navigationProjectId: PROJECT_ID,
    operationScope: scope,
    owner: OWNER,
    page: intro,
    pages: [intro, runtime],
    repoD: REPO_D,
    repoName: "Crew",
    snapshot: null,
    toc,
    ...overrides,
  };
}

function mount(overrides = {}) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  const view = render(
    React.createElement(
      QueryClientProvider,
      { client },
      React.createElement(WikiPageView, viewProps(overrides)),
    ),
  );
  return { client, view };
}

beforeEach(() => {
  cleanup();
  dom.window.localStorage.clear();
});

after(() => {
  cleanup();
  dom.window.close();
});

test("Wiki restores the scoped page and scroll position after remount", async () => {
  writeWikiNavigationState(identity(), {
    pageId: runtime.event.id,
    pageSlug: runtime.slug,
    scrollTop: 137,
  });
  const first = mount();
  try {
    await waitFor(() => screen.getByText("Runtime body"));
    const scroll = screen.getByTestId("wiki-page-scroll");
    assert.equal(scroll.scrollTop, 137);
    scroll.scrollTop = 241;
    fireEvent.scroll(scroll);
    assert.equal(readWikiNavigationState(identity()).scrollTop, 241);
  } finally {
    await act(async () => first.view.unmount());
    first.client.clear();
    first.client.unmount();
  }

  const second = mount();
  try {
    await waitFor(() => screen.getByText("Runtime body"));
    assert.equal(screen.getByTestId("wiki-page-scroll").scrollTop, 241);
  } finally {
    await act(async () => second.view.unmount());
    second.client.clear();
    second.client.unmount();
  }
});

test("Wiki restores the default page scroll after an unmount", async () => {
  writeWikiNavigationState(identity(), {
    pageId: intro.event.id,
    pageSlug: intro.slug,
    scrollTop: 137,
  });
  const first = mount();
  try {
    await waitFor(() => screen.getByText("Intro body"));
    assert.equal(screen.getByTestId("wiki-page-scroll").scrollTop, 137);
  } finally {
    await act(async () => first.view.unmount());
    first.client.clear();
    first.client.unmount();
  }

  const second = mount();
  try {
    await waitFor(() => screen.getByText("Intro body"));
    assert.equal(screen.getByTestId("wiki-page-scroll").scrollTop, 137);
  } finally {
    await act(async () => second.view.unmount());
    second.client.clear();
    second.client.unmount();
  }
});

test("Wiki persists a page selection before immediate unmount", async () => {
  writeWikiNavigationState(identity({}), {
    pageId: runtime.event.id,
    pageSlug: runtime.slug,
    scrollTop: 287,
  });
  const mounted = mount();
  try {
    await waitFor(() => screen.getByText("Runtime body"));
    await act(async () => {
      fireEvent.click(screen.getByTestId("wiki-toc-intro"));
    });
  } finally {
    await act(async () => mounted.view.unmount());
    mounted.client.clear();
    mounted.client.unmount();
  }
  assert.deepEqual(readWikiNavigationState(identity()), {
    pageId: intro.event.id,
    pageSlug: intro.slug,
    scrollTop: 0,
  });
  const remounted = mount();
  try {
    await waitFor(() => screen.getByText("Intro body"));
    assert.equal(screen.getByTestId("wiki-page-scroll").scrollTop, 0);
  } finally {
    await act(async () => remounted.view.unmount());
    remounted.client.clear();
    remounted.client.unmount();
  }
});

test("Wiki scope and project switches keep navigation snapshots isolated", async () => {
  const nextIdentity = identity({
    community: nextScope.scope.community,
    projectId: "project-2",
  });
  writeWikiNavigationState(identity(), {
    pageId: intro.event.id,
    pageSlug: intro.slug,
    scrollTop: 211,
  });
  writeWikiNavigationState(nextIdentity, {
    pageId: runtime.event.id,
    pageSlug: runtime.slug,
    scrollTop: 73,
  });
  const mounted = mount();
  try {
    await waitFor(() => screen.getByText("Intro body"));
    const scroll = screen.getByTestId("wiki-page-scroll");
    assert.equal(scroll.scrollTop, 211);
    await act(async () => {
      mounted.view.rerender(
        React.createElement(
          QueryClientProvider,
          { client: mounted.client },
          React.createElement(
            WikiPageView,
            viewProps({
              navigationProjectId: "project-2",
              operationScope: nextScope,
            }),
          ),
        ),
      );
    });
    await waitFor(() => screen.getByText("Runtime body"));
    assert.equal(screen.getByTestId("wiki-page-scroll").scrollTop, 73);
  } finally {
    await act(async () => mounted.view.unmount());
    mounted.client.clear();
    mounted.client.unmount();
  }
  assert.equal(readWikiNavigationState(identity()).scrollTop, 211);
  assert.equal(readWikiNavigationState(nextIdentity).scrollTop, 73);
});

test("Wiki falls back to a surviving page with a clear notice when the saved page is gone", async () => {
  writeWikiNavigationState(identity(), {
    pageId: "f".repeat(64),
    pageSlug: "gone",
    scrollTop: 90,
  });
  const mounted = mount();
  try {
    await waitFor(() => screen.getByTestId("wiki-navigation-fallback"));
    assert.match(
      screen.getByTestId("wiki-navigation-fallback").textContent,
      /no longer available/u,
    );
    assert.ok(screen.getByText("Intro body"));
    assert.equal(readWikiNavigationState(identity()).pageSlug, "intro");
  } finally {
    await act(async () => mounted.view.unmount());
    mounted.client.clear();
    mounted.client.unmount();
  }
});

test("Wiki keeps a selected page when publication refresh changes its event id", async () => {
  writeWikiNavigationState(identity(), {
    pageId: runtime.event.id,
    pageSlug: runtime.slug,
    scrollTop: 90,
  });
  const replacement = page(
    "3".repeat(64),
    runtime.slug,
    "Replacement runtime",
    "Replacement body",
  );
  const mounted = mount({
    page: intro,
    pages: [intro, replacement],
  });
  try {
    await waitFor(() => screen.getByText("Replacement body"));
    assert.equal(screen.queryByTestId("wiki-navigation-fallback"), null);
    assert.equal(
      readWikiNavigationState(identity()).pageId,
      replacement.event.id,
    );
    assert.equal(readWikiNavigationState(identity()).pageSlug, runtime.slug);
  } finally {
    await act(async () => mounted.view.unmount());
    mounted.client.clear();
    mounted.client.unmount();
  }
});

test("Wiki keeps the active non-first page through an in-place publication refresh", async () => {
  const initialIntro = v1Page(
    "5".repeat(64),
    `p1-${"a".repeat(64)}`,
    "intro",
    "Introduction",
    "Intro body",
  );
  const initialRuntime = v1Page(
    "6".repeat(64),
    `p1-${"b".repeat(64)}`,
    "runtime",
    "Runtime",
    "Runtime body",
  );
  const initialToc = tocForPages([initialIntro, initialRuntime]);
  const mounted = mount({
    page: initialIntro,
    pages: [initialIntro, initialRuntime],
    toc: initialToc,
  });
  try {
    await waitFor(() => screen.getByText("Intro body"));
    await act(async () => {
      fireEvent.click(screen.getByTestId(`wiki-toc-${initialRuntime.slug}`));
    });
    await waitFor(() => screen.getByText("Runtime body"));

    const refreshedIntro = v1Page(
      "3".repeat(64),
      `p1-${"c".repeat(64)}`,
      "intro",
      "Refreshed introduction",
      "Refreshed intro body",
    );
    const refreshedRuntime = v1Page(
      "4".repeat(64),
      `p1-${"d".repeat(64)}`,
      "runtime",
      "Refreshed runtime",
      "Refreshed runtime body",
    );
    const refreshedToc = tocForPages([refreshedIntro, refreshedRuntime]);
    await act(async () => {
      mounted.view.rerender(
        React.createElement(
          QueryClientProvider,
          { client: mounted.client },
          React.createElement(
            WikiPageView,
            viewProps({
              page: refreshedIntro,
              pages: [refreshedIntro, refreshedRuntime],
              toc: refreshedToc,
            }),
          ),
        ),
      );
    });
    await waitFor(() => screen.getByText("Refreshed runtime body"));
    assert.equal(screen.queryByTestId("wiki-navigation-fallback"), null);
    assert.equal(
      readWikiNavigationState(identity()).pageId,
      refreshedRuntime.event.id,
    );
    assert.equal(
      readWikiNavigationState(identity()).pageSlug,
      refreshedRuntime.logicalSlug,
    );
    assert.equal(refreshedRuntime.slug, `p1-${"d".repeat(64)}`);
    assert.equal(refreshedRuntime.logicalSlug, "runtime");
    assert.ok(
      screen.getByTestId(`wiki-toc-${refreshedRuntime.slug}`),
      "the TOC retains the current immutable address",
    );
  } finally {
    await act(async () => mounted.view.unmount());
    mounted.client.clear();
    mounted.client.unmount();
  }
});

test("Wiki keeps repository identity when its name is Company Wiki", async () => {
  const calls = [];
  const bridge = {
    invoke: async (command, args) => {
      calls.push({ command, args });
      if (command === "owner_operation_scope") return scope;
      if (command === "wiki_search") {
        return {
          token: scope,
          value: { events: [], truncated: false },
        };
      }
      throw new Error(`Unexpected Tauri command: ${command}`);
    },
    transformCallback: () => 1,
  };
  globalThis.__TAURI_INTERNALS__ = bridge;
  dom.window.__TAURI_INTERNALS__ = bridge;
  writeWikiNavigationState(identity(), {
    pageId: runtime.event.id,
    pageSlug: runtime.slug,
    scrollTop: 137,
  });
  const mounted = mount({
    isCompany: false,
    repoName: "Company Wiki",
    snapshot: verifiedSnapshot,
  });
  try {
    await waitFor(() => screen.getByText("Runtime body"));
    assert.deepEqual(readWikiNavigationState(identity()), {
      pageId: runtime.event.id,
      pageSlug: runtime.slug,
      scrollTop: 137,
    });
    const source = screen.getByTestId("wiki-source-identity");
    assert.equal(source.getAttribute("data-owner"), OWNER);
    assert.equal(source.getAttribute("data-repo-d"), REPO_D);
    assert.equal(source.getAttribute("data-coordinate"), COORDINATE);
    assert.equal(source.getAttribute("data-community"), COMMUNITY);

    fireEvent.change(screen.getByTestId("wiki-page-search"), {
      target: { value: "Intro" },
    });
    await waitFor(() => {
      assert.ok(
        calls.some(({ command }) => command === "wiki_search"),
        "repository body search must reach the native search seam",
      );
    });
    const search = calls.find(({ command }) => command === "wiki_search");
    assert.deepEqual(search?.args, {
      expected: scope,
      coordinate: COORDINATE,
      snapshotId: SNAPSHOT,
      pageIds: [intro.event.id, runtime.event.id],
      query: "Intro",
    });
  } finally {
    await act(async () => mounted.view.unmount());
    mounted.client.clear();
    mounted.client.unmount();
    delete globalThis.__TAURI_INTERNALS__;
    delete dom.window.__TAURI_INTERNALS__;
  }
});
