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
        source: "export const WikiSourceFiles = () => null;\n",
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
const { QueryClient, QueryClientProvider } = await import(
  "@tanstack/react-query"
);
const { readWikiNavigationState, writeWikiNavigationState } = await import(
  "../lib/wikiNavigationState.ts"
);
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

const intro = page("1".repeat(64), "intro", "Introduction", "Intro body");
const runtime = page("2".repeat(64), "runtime", "Runtime", "Runtime body");
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
