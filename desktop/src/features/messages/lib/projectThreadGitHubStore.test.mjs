import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { afterEach, beforeEach, test } from "node:test";
import { fileURLToPath } from "node:url";

class EventTargetShim {
  listeners = new Map();
  addEventListener(type, listener) {
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), listener]);
  }
  removeEventListener(type, listener) {
    this.listeners.set(
      type,
      (this.listeners.get(type) ?? []).filter(
        (current) => current !== listener,
      ),
    );
  }
  dispatchEvent(event) {
    for (const listener of this.listeners.get(event.type) ?? [])
      listener(event);
    return true;
  }
}

class ElementShim extends EventTargetShim {
  constructor() {
    super();
    this.children = [];
    this.childNodes = [];
    this.isContentEditable = false;
    this.nodeName = "DIV";
    this.tagName = "DIV";
    this.nodeType = 1;
    this.namespaceURI = "http://www.w3.org/1999/xhtml";
    this.style = {};
  }
  get ownerDocument() {
    return globalThis.document;
  }
  appendChild(child) {
    this.children.push(child);
    this.childNodes.push(child);
    return child;
  }
  removeChild(child) {
    this.children = this.children.filter((current) => current !== child);
    this.childNodes = this.childNodes.filter((current) => current !== child);
    return child;
  }
  insertBefore(child, reference) {
    const index = this.children.indexOf(reference);
    if (index < 0) return this.appendChild(child);
    this.children.splice(index, 0, child);
    this.childNodes.splice(index, 0, child);
    return child;
  }
}

globalThis.document = {
  addEventListener() {},
  createElement: () => new ElementShim(),
  createTextNode(value) {
    const node = new ElementShim();
    node.nodeType = 3;
    node.nodeValue = value;
    return node;
  },
  get defaultView() {
    return globalThis;
  },
  nodeType: 9,
  removeEventListener() {},
  get activeElement() {
    return null;
  },
};
globalThis.HTMLElement = ElementShim;
globalThis.HTMLDivElement = ElementShim;
globalThis.HTMLIFrameElement = ElementShim;
globalThis.Node = ElementShim;
globalThis.IS_REACT_ACT_ENVIRONMENT = true;
process.env.IS_REACT_ACT_ENVIRONMENT = "true";
Object.defineProperty(globalThis, "window", {
  configurable: true,
  value: globalThis,
});

import { createElement, useEffect, useRef } from "react";
import { act } from "react";
import { createRoot } from "react-dom/client";

import {
  resetProjectThreadGitHubStore,
  setProjectThreadGitHubRetryDelayForTests,
  setProjectThreadGitHubFetcherForTests,
  useProjectThreadGitHub,
} from "./projectThreadGitHubStore.ts";

const here = dirname(fileURLToPath(import.meta.url));

const target = {
  branch: "buzz/aaaaaaaaaaaa",
  repositoryPath: "/tmp/crew",
  rootEventId: "a".repeat(64),
};

beforeEach(() => {
  resetProjectThreadGitHubStore();
  setProjectThreadGitHubFetcherForTests(null);
});

afterEach(() => {
  setProjectThreadGitHubRetryDelayForTests(null);
  setProjectThreadGitHubFetcherForTests(null);
  resetProjectThreadGitHubStore();
});

function mockGitHubStatus() {
  let calls = 0;
  setProjectThreadGitHubFetcherForTests(async () => {
    calls += 1;
    return { availability: "available", pullRequest: null };
  });
  return {
    get calls() {
      return calls;
    },
  };
}

async function flush() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

/**
 * Mirrors ProjectThreadWorkspacePanel's drawer-refresh effect: depend on the
 * stable `refresh` callback, not a fresh model object each render (#34).
 * Returns null (no host DOM) so React only runs effects.
 */
function StableRefreshHarness({ activeDrawer, bump }) {
  const { refresh } = useProjectThreadGitHub(target);
  const seen = useRef(bump);
  seen.current = bump;
  useEffect(() => {
    if (
      activeDrawer === "issue" ||
      activeDrawer === "pr" ||
      activeDrawer === "ci"
    ) {
      void refresh();
    }
  }, [activeDrawer, refresh]);
  return null;
}

function SnapshotHarness({ onSnapshot }) {
  const { snapshot } = useProjectThreadGitHub(target);
  useEffect(() => {
    onSnapshot(snapshot);
  }, [onSnapshot, snapshot]);
  return null;
}

test("transient degraded GitHub status revalidates and recovers", async () => {
  let calls = 0;
  setProjectThreadGitHubRetryDelayForTests(0);
  setProjectThreadGitHubFetcherForTests(async () => {
    calls += 1;
    return calls === 1
      ? { availability: "cli-failed", pullRequest: null }
      : { availability: "available", pullRequest: null };
  });
  const snapshots = [];
  const root = createRoot(new ElementShim());

  await act(async () => {
    root.render(
      createElement(SnapshotHarness, {
        onSnapshot: (snapshot) => {
          snapshots.push(snapshot);
        },
      }),
    );
  });
  await new Promise((resolve) => setTimeout(resolve, 5));
  await flush();

  assert.equal(calls, 2, "degraded status should schedule one revalidation");
  assert.equal(
    snapshots.at(-1)?.value?.availability,
    "available",
    "successful retry should replace the stale degraded snapshot",
  );

  await act(async () => root.unmount());
});

test("rejected probe details are bounded, flattened, and redact tokens", async () => {
  const snapshots = [];
  setProjectThreadGitHubRetryDelayForTests(60_000);
  setProjectThreadGitHubFetcherForTests(async () => {
    throw new Error(`line one\ngho_secretvalue ${"x".repeat(300)}`);
  });
  const root = createRoot(new ElementShim());

  await act(async () => {
    root.render(
      createElement(SnapshotHarness, {
        onSnapshot: (snapshot) => {
          snapshots.push(snapshot);
        },
      }),
    );
  });
  await flush();

  const detail = snapshots.at(-1)?.value?.detail;
  assert.ok(detail, "degraded snapshot should retain a safe diagnostic");
  assert.equal(detail.includes("\n"), false);
  assert.equal(detail.includes("secretvalue"), false);
  assert.ok(Array.from(detail).length <= 241, "detail is capped plus ellipsis");

  await act(async () => root.unmount());
});

test("stable refresh dependency does not re-invoke after parent re-render", async () => {
  const status = mockGitHubStatus();
  const root = createRoot(new ElementShim());

  await act(async () => {
    root.render(
      createElement(StableRefreshHarness, { activeDrawer: "pr", bump: 0 }),
    );
  });
  await flush();

  const afterOpen = status.calls;
  assert.ok(afterOpen >= 1, "drawer open triggers at least one status load");
  assert.ok(
    afterOpen <= 2,
    `expected ≤2 invokes on open (mount + force refresh), got ${afterOpen}`,
  );

  await act(async () => {
    root.render(
      createElement(StableRefreshHarness, { activeDrawer: "pr", bump: 1 }),
    );
  });
  await flush();

  assert.equal(
    status.calls,
    afterOpen,
    "parent re-render must not fire another get_thread_github_status",
  );

  await act(async () => {
    root.unmount();
  });
});

test("panel effect keys on refreshGitHub; model hook memoizes its return", () => {
  const panel = readFileSync(
    join(here, "../ui/ProjectThreadWorkspacePanel.tsx"),
    "utf8",
  );
  const model = readFileSync(
    join(here, "../ui/useProjectThreadWorkspaceModel.ts"),
    "utf8",
  );
  assert.match(panel, /const refreshGitHub = model\?\.refreshGitHub/);
  assert.match(panel, /\[activeDrawer, refreshGitHub\]/);
  assert.equal(
    panel.includes("[activeDrawer, model]"),
    false,
    "effect must not depend on the whole model object",
  );
  assert.match(model, /return React\.useMemo\(\(\) => \{/);
});
