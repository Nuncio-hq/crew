import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import { test } from "node:test";

// Exercise the production mutation. Only external IO and React Query's hook
// shell are replaced; generation, publication ordering and failure policy run.
const stubs = new Map([
  [
    "@tanstack/react-query",
    "export const useMutation = x => x; export const useQueryClient = () => globalThis.wikiTest.query;",
  ],
  [
    "@tauri-apps/api/core",
    "export const invoke = (...x) => globalThis.wikiTest.invoke(...x);",
  ],
  [
    "@/features/wiki/hooks/useWikiEventsQuery",
    "export const wikiEventsQueryKey = ['crew-wiki-events'];",
  ],
  [
    "@/features/wiki/lib/wikiStore",
    "export const setWikiJob = x => globalThis.wikiTest.jobs.push(x);",
  ],
  [
    "@/shared/api/tauri",
    "export const signRelayEvent = async x => ({...x, id: String(++globalThis.wikiTest.signed)});",
  ],
  [
    "@/shared/api/relayClient",
    "export const relayClient = {publishEvent: (...x) => globalThis.wikiTest.publish(...x)};",
  ],
  [
    "@/shared/constants/kinds",
    "export const KIND_REPO_WIKI_PAGE = 30623; export const KIND_LONG_FORM = 30023; export const KIND_REPO_STATE = 30618;",
  ],
]);
registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier === "@/features/wiki/lib/wikiGenerationBatch")
      return {
        shortCircuit: true,
        url: new URL("../lib/wikiGenerationBatch.ts", import.meta.url).href,
      };
    if (stubs.has(specifier))
      return { shortCircuit: true, url: `wiki-test:${specifier}` };
    return nextResolve(specifier, context);
  },
  load(url, context, nextLoad) {
    if (url.startsWith("wiki-test:"))
      return {
        shortCircuit: true,
        format: "module",
        source: stubs.get(url.slice(10)),
      };
    return nextLoad(url, context);
  },
});
const { useWikiGenerate } = await import("./useWikiGenerate.ts");
const input = {
  owner: "a".repeat(64),
  repoD: "crew",
  repoKey: `${"a".repeat(64)}:crew`,
};
function fixture(outcome, failAt = 0) {
  const state = {
    jobs: [],
    signed: 0,
    writes: [],
    invalidations: 0,
    invoke: async () => {
      if (outcome instanceof Error) throw outcome;
      return outcome;
    },
    publish: async (event) => {
      state.writes.push(event);
      if (state.writes.length === failAt) throw new Error("ACK unknown");
    },
    query: {
      invalidateQueries: async () => {
        state.invalidations++;
      },
    },
  };
  globalThis.wikiTest = state;
  return state;
}
function batch() {
  const commit = "b".repeat(40);
  const tags = (slug) => [
    ["d", `crew/${slug}`],
    ["a", `30617:${input.owner}:crew`],
    ["commit", commit],
  ];
  return {
    accepted: true,
    pages: 1,
    commit,
    tocContent: JSON.stringify({
      sections: [
        {
          id: "overview",
          title: "Overview",
          pages: [{ slug: "overview", title: "Overview" }],
        },
      ],
    }),
    tocTags: tags("_toc"),
    drafts: [
      {
        slug: "overview",
        content: "real generated prose",
        tags: tags("overview"),
      },
    ],
  };
}
for (const [name, outcome] of [
  ["invoke failure", new Error("runtime failed")],
  ["empty result", { accepted: true, pages: 0 }],
  ["rejected generation", { ...batch(), accepted: false }],
  ["missing pages", { ...batch(), drafts: [] }],
  ["malformed TOC", { ...batch(), tocContent: "{broken" }],
  ["empty TOC", { ...batch(), tocContent: '{"sections":[]}' }],
  [
    "blank page",
    { ...batch(), drafts: [{ ...batch().drafts[0], content: "  " }] },
  ],
  ["wrong repository", { ...batch(), tocTags: [["d", "other/_toc"]] }],
  [
    "wrong page identity",
    {
      ...batch(),
      drafts: [{ ...batch().drafts[0], tags: [["d", "other/overview"]] }],
    },
  ],
  ["wrong count", { ...batch(), pages: 2 }],
]) {
  test(`${name} remains failed without publishing invented success`, async () => {
    const state = fixture(outcome);
    await assert.rejects(useWikiGenerate().mutationFn(input));
    assert.equal(state.writes.length, 0);
    assert.equal(state.jobs.at(-1).status, "failed");
    assert.ok(state.jobs.at(-1).error);
  });
}
test("unknown publication ACK propagates without fallback or success", async () => {
  const state = fixture(batch(), 1);
  await assert.rejects(useWikiGenerate().mutationFn(input), /ACK unknown/);
  assert.equal(state.writes.length, 1);
  assert.equal(state.jobs.at(-1).status, "failed");
});
test("explicit missing workspace does not publish", async () => {
  const state = fixture({ missingLocalPath: true });
  await useWikiGenerate().mutationFn(input);
  assert.equal(state.writes.length, 0);
  assert.equal(state.jobs.at(-1).error, "missing-local-path");
});

for (const failAt of [1, 2]) {
  test(`publish ${failAt} failure stops that batch without synthetic retries`, async () => {
    const state = fixture(batch(), failAt);
    await assert.rejects(useWikiGenerate().mutationFn(input), /ACK unknown/);
    assert.equal(state.writes.length, failAt);
    assert.equal(state.jobs.at(-1).error, "ACK unknown");
  });
}
test("valid explicit generation still publishes its real content", async () => {
  const state = fixture(batch());
  await useWikiGenerate().mutationFn(input);
  assert.equal(state.writes.length, 2);
  assert.ok(
    state.writes.some((event) => event.content === "real generated prose"),
  );
  assert.equal(state.jobs.at(-1).error, null);
});

const { jobForRepo } = await import("../lib/wikiEvents.ts");
test("same d-tag under another owner cannot supply this repository's job", () => {
  const otherKey = `${"c".repeat(64)}:crew`;
  const other = {
    repoKey: otherKey,
    status: "failed",
    error: "other owner failure",
  };
  assert.equal(
    jobForRepo(new Map([[otherKey, other]]), input.owner, "crew"),
    undefined,
  );
});
test("exact repository job remains available", () => {
  const job = {
    repoKey: input.repoKey,
    status: "failed",
    error: "own failure",
  };
  assert.equal(
    jobForRepo(new Map([[input.repoKey, job]]), input.owner, "crew"),
    job,
  );
});

const EVENT_CONTENT_LIMIT = 192 * 1024;
for (const [name, body] of [
  ["ASCII", "x".repeat(EVENT_CONTENT_LIMIT + 1)],
  ["multibyte", "ế".repeat(EVENT_CONTENT_LIMIT / 3 + 1)],
]) {
  test(`oversized ${name} page fails before TOC signing or publication`, async () => {
    const outcome = batch();
    outcome.drafts[0].content = body;
    const state = fixture(outcome);
    await assert.rejects(useWikiGenerate().mutationFn(input));
    assert.equal(state.signed, 0);
    assert.equal(state.writes.length, 0);
  });
  test(`oversized ${name} TOC fails before signing or publication`, async () => {
    const outcome = batch();
    const toc = JSON.parse(outcome.tocContent);
    outcome.tocContent = JSON.stringify({ ...toc, note: body });
    const state = fixture(outcome);
    await assert.rejects(useWikiGenerate().mutationFn(input));
    assert.equal(state.signed, 0);
    assert.equal(state.writes.length, 0);
  });
}
for (const [name, body] of [
  ["ASCII", "x".repeat(EVENT_CONTENT_LIMIT - 1024)],
  ["multibyte", "ế".repeat(Math.floor((EVENT_CONTENT_LIMIT - 1024) / 3))],
]) {
  test(`near-limit ${name} page and TOC remain valid`, async () => {
    const outcome = batch();
    outcome.drafts[0].content = body;
    const toc = JSON.parse(outcome.tocContent);
    outcome.tocContent = JSON.stringify({ ...toc, note: body });
    const state = fixture(outcome);
    await useWikiGenerate().mutationFn(input);
    assert.equal(state.writes.length, 2);
    assert.equal(state.jobs.at(-1).error, null);
  });
}
