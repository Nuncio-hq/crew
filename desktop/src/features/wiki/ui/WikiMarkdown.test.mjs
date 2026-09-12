import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import { after, beforeEach, test } from "node:test";
import { JSDOM } from "jsdom";

let delegated = [];

registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier === "@/shared/ui/markdown") {
      return { shortCircuit: true, url: "crew-wiki-markdown-stub:markdown" };
    }
    if (specifier === "@/shared/ui/markdown/entityLinks") {
      return {
        shortCircuit: true,
        url: "crew-wiki-markdown-stub:entity-links",
      };
    }
    return nextResolve(specifier, context);
  },
  load(url, context, nextLoad) {
    if (url === "crew-wiki-markdown-stub:markdown") {
      return {
        format: "module",
        shortCircuit: true,
        source: `
          export const Markdown = (props) => {
            globalThis.__WIKI_MARKDOWN_PROPS__ = props;
            return null;
          };
        `,
      };
    }
    if (url === "crew-wiki-markdown-stub:entity-links") {
      return {
        format: "module",
        shortCircuit: true,
        source: `
          export const useOpenEntityLink = () => (link) => {
            globalThis.__WIKI_DELEGATED__?.push(link);
          };
        `,
      };
    }
    return nextLoad(url, context);
  },
});

const OWNER = "a".repeat(64);
const REPO_D = "crew";
const SOURCE_PATH = "src/runtime.ts";
const pageEvent = {
  id: "f".repeat(64),
  pubkey: OWNER,
  kind: 30623,
  content: "Wiki page",
  created_at: 10,
  sig: "",
  tags: [
    [
      "wiki-source-files",
      JSON.stringify([[SOURCE_PATH, "d".repeat(64), 2, 2, 3]]),
    ],
  ],
};

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});
Object.assign(globalThis, {
  document: dom.window.document,
  HTMLElement: dom.window.HTMLElement,
  IS_REACT_ACT_ENVIRONMENT: true,
  window: dom.window,
  __WIKI_DELEGATED__: delegated,
});

const { act, cleanup, render } = await import("@testing-library/react");
const React = await import("react");
const { WikiMarkdown, rewriteRelativeFileLinks } = await import(
  "./WikiMarkdown.tsx"
);

function mount(props = {}) {
  return render(
    React.createElement(WikiMarkdown, {
      owner: OWNER,
      pageEvent,
      repoD: REPO_D,
      source: "See [runtime](buzz://file?path=src%2Fruntime.ts&lines=2-3).",
      ...props,
    }),
  );
}

beforeEach(() => {
  cleanup();
  delegated = [];
  globalThis.__WIKI_DELEGATED__ = delegated;
});

after(() => {
  cleanup();
  dom.window.close();
});

test("Wiki Markdown routes an exact file citation to the verified source opener", () => {
  const opened = [];
  const unavailable = [];
  const trigger = document.createElement("button");
  document.body.append(trigger);
  const view = mount({
    onOpenSource: (request, sourceTrigger) =>
      opened.push({ request, sourceTrigger }),
    onSourceUnavailable: (message) => unavailable.push(message),
  });
  try {
    globalThis.__WIKI_LINK__ = {
      type: "file",
      owner: OWNER,
      dtag: REPO_D,
      path: SOURCE_PATH,
      startLine: 2,
      endLine: 3,
    };
    globalThis.__WIKI_MARKDOWN_PROPS__.onOpenEntityLink(
      globalThis.__WIKI_LINK__,
      trigger,
    );
    assert.deepEqual(opened, [
      {
        request: { path: SOURCE_PATH, startLine: 2, endLine: 3 },
        sourceTrigger: trigger,
      },
    ]);
    assert.deepEqual(unavailable, []);
    assert.equal(
      globalThis.__WIKI_MARKDOWN_PROPS__.content.includes("owner="),
      true,
    );
  } finally {
    act(() => view.unmount());
    trigger.remove();
  }
});

test("Wiki Markdown refuses a file citation outside the current page coordinate", () => {
  const opened = [];
  const unavailable = [];
  const view = mount({
    onOpenSource: (request) => opened.push(request),
    onSourceUnavailable: (message) => unavailable.push(message),
  });
  try {
    globalThis.__WIKI_LINK__ = {
      type: "file",
      owner: "b".repeat(64),
      dtag: REPO_D,
      path: SOURCE_PATH,
      startLine: 2,
      endLine: 3,
    };
    globalThis.__WIKI_MARKDOWN_PROPS__.onOpenEntityLink(
      globalThis.__WIKI_LINK__,
    );
    assert.deepEqual(opened, []);
    assert.equal(unavailable.length, 1);
    assert.deepEqual(globalThis.__WIKI_DELEGATED__, []);
  } finally {
    act(() => view.unmount());
  }
});

test("relative citation rewriting preserves malformed percent escapes", () => {
  const source = "[broken](buzz://file?path=%E0%A4&lines=2-3)";
  assert.doesNotThrow(() => rewriteRelativeFileLinks(source, OWNER, REPO_D));
  assert.equal(rewriteRelativeFileLinks(source, OWNER, REPO_D), source);
});
