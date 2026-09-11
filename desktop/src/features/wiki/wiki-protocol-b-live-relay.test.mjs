import assert from "node:assert/strict";
import { createHash, randomUUID } from "node:crypto";
import { test } from "node:test";

import { hexToBytes } from "@noble/hashes/utils.js";
import { Relay } from "nostr-tools/relay";
import { finalizeEvent, getPublicKey, verifyEvent } from "nostr-tools/pure";

import {
  MAX_EVENT_BYTES,
  MAX_GRAPH_BYTES,
  MAX_PUBLICATION_PAGES,
  PAGE_QUERY_BATCH,
  assertFilterBudget,
  createGraphBudget,
  createResponseCollector,
  envelopeBytes,
  readBoundedJson,
  responseEventCap,
} from "./wiki-protocol-b-budget.mjs";

// This test is deliberately opt-in. The owner key and repository coordinate
// must be disposable values supplied by the operator; the test never guesses
// a relay, owner, or repository that could belong to a real workspace.
const ORIGIN = process.env.CREW_PROTOCOL_B_ORIGIN;
const OWNER_SECRET_KEY = process.env.CREW_PROTOCOL_B_OWNER_SECRET_KEY;
const EXPECTED_OWNER = process.env.CREW_PROTOCOL_B_OWNER;
const REPO_D = process.env.CREW_PROTOCOL_B_REPO_D;
const EXPECTED_NATIVE_HEAD_ID =
  process.env.CREW_PROTOCOL_B_EXPECTED_NATIVE_HEAD_ID;
const SOURCE_TEXT = "# Protocol B\n\nDisposable live relay acceptance.\n";
const REPOSITORY_KIND = 30_617;
const SOURCE_REVISION = `git:${"a".repeat(40)}`;
const WIKI_KIND = 30_623;

function requireConfig() {
  if (!ORIGIN || !OWNER_SECRET_KEY || !REPO_D) return null;
  assert.match(ORIGIN, /^wss?:\/\//, "CREW_PROTOCOL_B_ORIGIN must be ws(s)://");
  assert.match(
    OWNER_SECRET_KEY,
    /^[0-9a-f]{64}$/,
    "CREW_PROTOCOL_B_OWNER_SECRET_KEY must be 32-byte lowercase hex",
  );
  assert.match(
    REPO_D,
    /^(?!\.)(?!.*\.\.)[A-Za-z0-9._-]{1,64}$/,
    "CREW_PROTOCOL_B_REPO_D must be a disposable NIP-34 repository identifier",
  );
  if (EXPECTED_NATIVE_HEAD_ID) {
    assert.match(
      EXPECTED_NATIVE_HEAD_ID,
      /^[0-9a-f]{64}$/,
      "CREW_PROTOCOL_B_EXPECTED_NATIVE_HEAD_ID must be a 32-byte lowercase hex event id",
    );
  }
  const secretKey = hexToBytes(OWNER_SECRET_KEY);
  const owner = getPublicKey(secretKey);
  if (EXPECTED_OWNER) assert.equal(EXPECTED_OWNER, owner);
  return { owner, secretKey };
}

function digest(value) {
  return createHash("sha256").update(JSON.stringify(value)).digest("hex");
}

function sourceHash(value) {
  return createHash("sha256").update(value).digest("hex");
}

function sourceReference() {
  return [
    "README.md",
    sourceHash(SOURCE_TEXT),
    Buffer.byteLength(SOURCE_TEXT),
    1,
    SOURCE_TEXT.split("\n").length - 1,
  ];
}

function commonTags(owner, repoD, snapshotId, slug) {
  return [
    ["d", `${repoD}/${slug}`],
    ["a", `30617:${owner}:${repoD}`],
    ["wiki-version", "1"],
    ["wiki-snapshot", snapshotId],
    ["source-kind", "git"],
    ["commit", SOURCE_REVISION.slice("git:".length)],
  ];
}

function buildRepositoryAnnouncement({ secretKey, repoD, createdAt }) {
  const event = finalizeEvent(
    {
      kind: REPOSITORY_KIND,
      created_at: createdAt,
      tags: [
        ["d", repoD],
        ["name", repoD],
      ],
      content: "Protocol B disposable repository anchor",
    },
    secretKey,
  );
  assert.equal(verifyEvent(event), true);
  return event;
}

function buildPublication({
  owner,
  secretKey,
  repoD,
  expectedRevision,
  text,
  createdAt,
}) {
  const snapshotId = randomUUID();
  const slug = "intro";
  const title = "Protocol B";
  const section = "overview";
  const language = "en";
  const sources = [sourceReference()];
  const envelope = [
    1,
    snapshotId,
    owner,
    repoD,
    SOURCE_REVISION,
    slug,
    title,
    section,
    language,
    sources,
    text,
  ];
  const pageDigest = digest(envelope);
  const pageSlug = `p1-${pageDigest}`;
  const page = finalizeEvent(
    {
      kind: WIKI_KIND,
      created_at: createdAt,
      tags: [
        ...commonTags(owner, repoD, snapshotId, pageSlug),
        ["wiki-slug", slug],
        ["title", title],
        ["section", section],
        ["language", language],
        ["wiki-source-files", JSON.stringify(sources)],
        ["source", "README.md"],
      ],
      content: text,
    },
    secretKey,
  );

  const sections = [[section, "Overview", [slug]]];
  const pageReference = [
    slug,
    pageSlug,
    page.id,
    pageDigest,
    title,
    section,
    language,
    sources,
  ];
  const manifest = [
    1,
    snapshotId,
    owner,
    repoD,
    SOURCE_REVISION,
    null,
    sections,
    [pageReference],
  ];
  const manifestDigest = digest(manifest);
  const manifestEvent = finalizeEvent(
    {
      kind: WIKI_KIND,
      created_at: createdAt,
      tags: commonTags(owner, repoD, snapshotId, `m1-${manifestDigest}`),
      content: JSON.stringify(manifest),
    },
    secretKey,
  );
  const projection = {
    sections: [
      {
        id: section,
        title: "Overview",
        pages: [{ slug: pageSlug, title }],
      },
    ],
  };
  const head = finalizeEvent(
    {
      kind: WIKI_KIND,
      created_at: createdAt,
      tags: [
        ...commonTags(owner, repoD, snapshotId, "_toc"),
        ["wiki-manifest", manifestEvent.id, manifestDigest],
        ["cadence", "manual"],
        ["expected-revision", expectedRevision],
      ],
      content: JSON.stringify(projection),
    },
    secretKey,
  );
  for (const event of [page, manifestEvent, head]) {
    assert.equal(verifyEvent(event), true);
    assert.equal(event.pubkey, owner);
  }
  return {
    head,
    manifest: manifestEvent,
    page,
    manifestId: manifestEvent.id,
    manifestDigest,
    pageSlug,
    snapshotId,
  };
}

async function connectRelay(url, secretKey) {
  const relay = new Relay(url, { enableReconnect: false });
  relay.onauth = async (template) => finalizeEvent(template, secretKey);
  await relay.connect();
  return relay;
}

async function assertConditionalCapability(url) {
  const descriptorUrl = url.replace(/^ws/, "http");
  const response = await fetch(descriptorUrl, {
    headers: { Accept: "application/nostr+json" },
    signal: AbortSignal.timeout(5_000),
  });
  assert.equal(response.ok, true, "the disposable relay must expose NIP-11");
  // `response.json()` would buffer whatever the origin sends. The descriptor
  // is a small trusted-shape document, so read it under an explicit cap.
  const descriptor = await readBoundedJson(response);
  assert.ok(
    descriptor.supported_extensions?.includes(
      "crew-conditional-publication-v1",
    ),
    "the disposable relay must advertise conditional Wiki publication",
  );
}

// Read one filter under both budgets: the serialized request stays inside the
// filter bound, and every returned event is measured against the per-event,
// per-response, and whole-graph bounds *before* this process retains it. A
// relay that keeps streaming past a bound gets its subscription closed and the
// read rejected rather than growing an unbounded array.
function fetchEvents(relay, filter, graph) {
  assertFilterBudget(filter);
  return new Promise((resolve, reject) => {
    const collector = createResponseCollector({
      maxEvents: responseEventCap(filter),
      graph,
    });
    let settled = false;
    let subscription;
    const fail = (error, reason) => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      subscription?.close(reason);
      reject(error);
    };
    const timeout = setTimeout(() => {
      fail(
        new Error("Protocol B live relay read timed out."),
        "protocol B read timed out",
      );
    }, 5_000);
    subscription = relay.subscribe([filter], {
      onevent: (event) => {
        if (settled) return;
        try {
          collector.admit(event);
        } catch (error) {
          fail(error, "protocol B response budget exceeded");
        }
      },
      oneose: () => {
        if (settled) return;
        settled = true;
        clearTimeout(timeout);
        subscription.close();
        resolve(collector.events);
      },
      onclose: (reason) => {
        if (settled) return;
        if (reason && reason !== "closed by caller") {
          settled = true;
          clearTimeout(timeout);
          reject(new Error(reason));
        }
      },
    });
  });
}

async function publish(relay, event) {
  await relay.publish(event);
  return event;
}

function tag(event, name) {
  const matches = event.tags.filter((value) => value[0] === name);
  assert.equal(matches.length, 1, `expected one ${name} tag`);
  return matches[0];
}

async function readFullSnapshot(relay, owner, repoD) {
  // One budget spans the whole graph: head, manifest, and every page batch.
  const graph = createGraphBudget();
  const headFilter = {
    kinds: [WIKI_KIND],
    authors: [owner],
    "#d": [`${repoD}/_toc`],
    limit: 2,
  };
  const headEvents = await fetchEvents(relay, headFilter, graph);
  assert.equal(
    headEvents.length,
    1,
    "fresh read must return one live Wiki head",
  );
  const head = headEvents[0];
  assert.equal(verifyEvent(head), true);
  assert.ok(
    envelopeBytes(head) <= MAX_EVENT_BYTES,
    "Wiki head exceeds 192 KiB",
  );
  const manifestTag = tag(head, "wiki-manifest");
  const manifestFilter = {
    kinds: [WIKI_KIND],
    authors: [owner],
    ids: [manifestTag[1]],
    "#d": [`${repoD}/m1-${manifestTag[2]}`],
    limit: 2,
  };
  const manifestEvents = await fetchEvents(relay, manifestFilter, graph);
  assert.equal(
    manifestEvents.length,
    1,
    "fresh read must return the exact manifest",
  );
  const manifest = manifestEvents[0];
  assert.equal(verifyEvent(manifest), true);
  assert.ok(
    envelopeBytes(manifest) <= MAX_EVENT_BYTES,
    "Wiki manifest exceeds 192 KiB",
  );
  assert.equal(manifest.id, manifestTag[1]);
  assert.equal(digest(JSON.parse(manifest.content)), manifestTag[2]);
  const parsed = JSON.parse(manifest.content);
  assert.equal(JSON.stringify(parsed), manifest.content);
  assert.equal(parsed[2], owner);
  assert.equal(parsed[3], repoD);
  assert.equal(tag(head, "d")[1], `${repoD}/_toc`);
  assert.equal(tag(head, "a")[1], `30617:${owner}:${repoD}`);
  assert.equal(tag(head, "wiki-snapshot")[1], parsed[1]);
  const references = parsed[7];
  assert.ok(Array.isArray(references) && references.length > 0);
  assert.ok(
    references.length <= graph.maxPages &&
      graph.maxPages === MAX_PUBLICATION_PAGES,
    "Wiki publication exceeds the 256-page bound",
  );
  const referencesBySlug = new Map(
    references.map((reference) => [reference[0], reference]),
  );
  assert.equal(
    head.content,
    JSON.stringify({
      sections: parsed[6].map(([id, title, slugs]) => ({
        id,
        title,
        pages: slugs.map((slug) => {
          const reference = referencesBySlug.get(slug);
          assert.ok(reference);
          return { slug: reference[1], title: reference[4] };
        }),
      })),
    }),
  );
  const pages = [];
  for (let offset = 0; offset < references.length; offset += PAGE_QUERY_BATCH) {
    const batch = references.slice(offset, offset + PAGE_QUERY_BATCH);
    const filter = {
      kinds: [WIKI_KIND],
      authors: [owner],
      ids: batch.map((reference) => reference[2]),
      "#d": batch.map((reference) => `${repoD}/${reference[1]}`),
      limit: batch.length + 1,
    };
    pages.push(...(await fetchEvents(relay, filter, graph)));
  }
  assert.equal(
    pages.length,
    references.length,
    "fresh read must return every page",
  );
  const byId = new Map(pages.map((page) => [page.id, page]));
  for (const reference of references) {
    const page = byId.get(reference[2]);
    assert.ok(page, `manifest page ${reference[0]} must be present`);
    assert.equal(verifyEvent(page), true);
    assert.ok(
      envelopeBytes(page) <= MAX_EVENT_BYTES,
      "Wiki page exceeds 192 KiB",
    );
    assert.equal(page.pubkey, owner);
    assert.equal(tag(page, "d")[1], `${repoD}/${reference[1]}`);
    assert.equal(tag(page, "wiki-snapshot")[1], parsed[1]);
    assert.equal(
      tag(page, "wiki-source-files")[1],
      JSON.stringify(reference[7]),
    );
    assert.equal(
      digest([
        1,
        parsed[1],
        owner,
        repoD,
        parsed[4],
        reference[0],
        reference[4],
        reference[5],
        reference[6],
        reference[7],
        page.content,
      ]),
      reference[3],
    );
    assert.equal(reference[1], `p1-${reference[3]}`);
  }
  const rereadHeadFilter = {
    kinds: [WIKI_KIND],
    authors: [owner],
    "#d": [`${repoD}/_toc`],
    limit: 2,
  };
  const rereadHead = await fetchEvents(relay, rereadHeadFilter, graph);
  assert.equal(
    rereadHead.length,
    1,
    "head reread must remain a single live row",
  );
  assert.equal(
    rereadHead[0].id,
    head.id,
    "a full graph read must be fenced to one head revision",
  );
  assert.ok(
    graph.bytes <= MAX_GRAPH_BYTES,
    "a whole Wiki graph read must stay within the 64 MiB retention budget",
  );
  return { head, manifest, pages, manifestValue: parsed, graph };
}

async function publishResult(relay, event) {
  try {
    await publish(relay, event);
    return { accepted: true, event };
  } catch (error) {
    return {
      accepted: false,
      event,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

test("Protocol B reads a complete Wiki snapshot and arbitrates concurrent CAS", {
  skip: !ORIGIN || !OWNER_SECRET_KEY || !REPO_D,
  timeout: 30_000,
}, async () => {
  const config = requireConfig();
  assert.ok(config);
  const { owner, secretKey } = config;
  let relay;
  try {
    await assertConditionalCapability(ORIGIN);
    relay = await connectRelay(ORIGIN, secretKey);
    const now = Math.floor(Date.now() / 1_000);
    let initial;
    if (EXPECTED_NATIVE_HEAD_ID) {
      // Native A acceptance mode starts from a graph created by the governed
      // producer. B must read that exact live head after reconnecting; it does
      // not seed a parallel fixture that could make its own reader look green.
      initial = { expectedHeadId: EXPECTED_NATIVE_HEAD_ID };
    } else {
      // Self-contained mode owns its disposable repository coordinate and
      // publishes the NIP-34 30617 anchor before the Wiki graph. A Wiki `a`
      // tag without this durable repository event is not an accepted graph.
      await publish(
        relay,
        buildRepositoryAnnouncement({
          secretKey,
          repoD: REPO_D,
          createdAt: now,
        }),
      );
      initial = buildPublication({
        owner,
        secretKey,
        repoD: REPO_D,
        expectedRevision: "absent",
        text: SOURCE_TEXT,
        createdAt: now,
      });
      await publish(relay, initial.page);
      await publish(relay, initial.manifest);
      await publish(relay, initial.head);
    }

    relay.close();
    relay = await connectRelay(ORIGIN, secretKey);
    const coldInitial = await readFullSnapshot(relay, owner, REPO_D);
    if (initial.expectedHeadId) {
      assert.equal(
        coldInitial.head.id,
        initial.expectedHeadId,
        "B must read the exact native A head after reconnecting",
      );
    } else {
      assert.equal(coldInitial.head.id, initial.head.id);
      assert.equal(coldInitial.manifest.id, initial.manifest.id);
      assert.equal(coldInitial.pages[0].id, initial.page.id);
    }
    const expectedRevision = coldInitial.head.id;

    const contenders = [
      buildPublication({
        owner,
        secretKey,
        repoD: REPO_D,
        expectedRevision,
        text: `${SOURCE_TEXT}winner A\n`,
        createdAt: now + 2,
      }),
      buildPublication({
        owner,
        secretKey,
        repoD: REPO_D,
        expectedRevision,
        text: `${SOURCE_TEXT}winner B\n`,
        createdAt: now + 3,
      }),
    ];
    for (const contender of contenders) {
      await publish(relay, contender.page);
      await publish(relay, contender.manifest);
    }
    const results = await Promise.all(
      contenders.map((contender) => publishResult(relay, contender.head)),
    );
    assert.equal(
      results.filter((result) => result.accepted).length,
      1,
      "exactly one concurrent expected-revision write must win",
    );
    assert.equal(
      results.filter((result) => !result.accepted).length,
      1,
      "the other concurrent expected-revision write must conflict",
    );
    const winner = results.find((result) => result.accepted)?.event;
    const loser = results.find((result) => !result.accepted);
    assert.ok(winner);
    assert.ok(loser?.error);
    assert.match(
      loser.error,
      /conflict: conditional publication revision changed/,
      "the losing CAS must return the relay's authoritative conflict",
    );

    await publish(relay, winner);
    relay.close();
    relay = await connectRelay(ORIGIN, secretKey);
    const coldWinner = await readFullSnapshot(relay, owner, REPO_D);
    assert.equal(
      coldWinner.head.id,
      winner.id,
      "exact live replay must retain the winner",
    );
    assert.equal(
      coldWinner.head.id,
      contenders.find((contender) => contender.head.id === winner.id).head.id,
    );
    assert.equal(
      coldWinner.manifest.id,
      contenders.find((contender) => contender.head.id === winner.id).manifest
        .id,
    );
    assert.equal(
      coldWinner.pages[0].id,
      contenders.find((contender) => contender.head.id === winner.id).page.id,
    );
    await assert.rejects(
      publish(relay, loser.event),
      /conflict: conditional publication revision changed/,
      "the superseded contender must not become live through replay",
    );
  } finally {
    relay?.close();
  }
});
