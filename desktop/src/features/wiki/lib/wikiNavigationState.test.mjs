import assert from "node:assert/strict";
import { after, beforeEach, test } from "node:test";

import {
  readWikiNavigationState,
  WIKI_NAVIGATION_STORAGE_KEY,
  writeWikiNavigationState,
} from "./wikiNavigationState.ts";

class MemoryStorage {
  #values = new Map();

  getItem(key) {
    return this.#values.get(key) ?? null;
  }

  setItem(key, value) {
    this.#values.set(key, String(value));
  }

  removeItem(key) {
    this.#values.delete(key);
  }

  clear() {
    this.#values.clear();
  }
}

const storage = new MemoryStorage();
Object.defineProperty(globalThis, "localStorage", {
  configurable: true,
  value: storage,
});

const identity = (overrides = {}) => ({
  community: "https://relay.example",
  viewer: "A".repeat(64),
  projectId: "project-1",
  repositoryCoordinate: `30617:${"a".repeat(64)}:crew`,
  surface: "project",
  ...overrides,
});

beforeEach(() => storage.clear());
after(() => {
  delete globalThis.localStorage;
});

test("Wiki navigation state is scoped by community, viewer, project and repository", () => {
  const current = identity();
  writeWikiNavigationState(current, {
    pageId: "page-1",
    pageSlug: "overview",
    scrollTop: 240,
  });

  assert.deepEqual(readWikiNavigationState(current), {
    pageId: "page-1",
    pageSlug: "overview",
    scrollTop: 240,
  });
  assert.equal(
    readWikiNavigationState(identity({ community: "https://other.example" })),
    null,
  );
  assert.equal(
    readWikiNavigationState(identity({ viewer: "b".repeat(64) })),
    null,
  );
  assert.equal(
    readWikiNavigationState(
      identity({ repositoryCoordinate: "30617:a:other" }),
    ),
    null,
  );
});

test("Wiki navigation state clamps malformed scroll values and bounds stored entries", () => {
  writeWikiNavigationState(identity(), {
    pageId: "page-1",
    pageSlug: "overview",
    scrollTop: Number.POSITIVE_INFINITY,
  });
  assert.equal(readWikiNavigationState(identity()).scrollTop, 0);

  for (let index = 0; index < 70; index += 1) {
    writeWikiNavigationState(identity({ projectId: `project-${index}` }), {
      pageId: `page-${index}`,
      pageSlug: `page-${index}`,
      scrollTop: index,
    });
  }
  const entries = JSON.parse(storage.getItem(WIKI_NAVIGATION_STORAGE_KEY));
  assert.equal(Object.keys(entries).length, 64);
});
