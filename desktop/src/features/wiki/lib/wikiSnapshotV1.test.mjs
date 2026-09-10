import { rewriteIndex } from "./wikiSnapshotV1.fixtures.mjs";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { finalizeEvent, verifyEvent } from "nostr-tools/pure";
import { verifyWikiSnapshotV1 } from "./wikiSnapshotV1.ts";
import { fixture, repoD, resign, repoint } from "./wikiSnapshotV1.fixtures.mjs";
const clone = (value) => JSON.parse(JSON.stringify(value));

test("complete real-signed snapshot is accepted with exact immutable bodies", () => {
  const input = fixture();
  const result = verifyWikiSnapshotV1(input);
  assert.ok(result);
  assert.equal(result.pages[0].id, input.pages[0].id);
  assert.equal(result.pages[0].content, input.pages[0].content);
});

test("verified output does not retain mutable renderer event references", () => {
  const input = fixture();
  const result = verifyWikiSnapshotV1(input);
  assert.ok(result);
  input.pages[0].content = "mutated after validation";
  input.head.tags[0][1] = "wrong";
  assert.notEqual(result.pages[0].content, input.pages[0].content);
  assert.equal(result.head.tags[0][1], `${repoD}/_toc`);
});

test("signature cache on a mutated signed object never authenticates it", () => {
  const input = fixture();
  assert.equal(verifyEvent(input.pages[0]), true);
  input.pages[0].sig = "0".repeat(128);
  assert.equal(verifyWikiSnapshotV1(input), null);
});

for (const [name, mutate] of [
  [
    "missing page",
    (x) => {
      x.pages = [];
    },
  ],
  [
    "extra duplicate page",
    (x) => {
      x.pages.push(x.pages[0]);
    },
  ],
  [
    "wrong owner scope",
    (x) => {
      x.owner = "c".repeat(64);
    },
  ],
  [
    "wrong repository scope",
    (x) => {
      x.repoD = "other";
    },
  ],
  [
    "body mutation",
    (x) => {
      x.pages[0].content += "modified";
    },
  ],
  [
    "extra a tag value",
    (x) => {
      x.head.tags.find((t) => t[0] === "a").push("ignored");
      x.head = resign(x.head);
    },
  ],
  [
    "duplicate required tag",
    (x) => {
      x.head.tags.push(["wiki-version", "1"]);
      x.head = resign(x.head);
    },
  ],
  [
    "wrong legacy projection",
    (x) => {
      x.head.content = x.head.content.replace("Overview", "Other");
      x.head = resign(x.head);
    },
  ],
  [
    "noncanonical head object",
    (x) => {
      x.head.content = JSON.stringify(JSON.parse(x.head.content), null, 2);
      x.head = resign(x.head);
    },
  ],
  [
    "mismatched source kind",
    (x) => {
      x.head.tags.find((t) => t[0] === "source-kind")[1] = "folder";
      x.head = resign(x.head);
    },
  ],
  [
    "unsigned renderer event",
    (x) => {
      delete x.head.sig;
    },
  ],
])
  test(`rejects ${name}`, () => {
    const x = clone(fixture());
    mutate(x);
    assert.equal(verifyWikiSnapshotV1(x), null);
  });

for (const [name, change] of [
  [
    "empty file byte count",
    (e) => {
      e[9][0][2] = 0;
      return e;
    },
  ],
  [
    "zero citation line",
    (e) => {
      e[9][0][3] = 0;
      return e;
    },
  ],
  [
    "reversed source range",
    (e) => {
      e[9][0][3] = 2;
      return e;
    },
  ],
  [
    "traversal path",
    (e) => {
      e[9][0][0] = "../secret";
      return e;
    },
  ],
  [
    "NUL body",
    (e) => {
      e[10] += "\0";
      return e;
    },
  ],
  [
    "empty title",
    (e) => {
      e[6] = " ";
      return e;
    },
  ],
])
  test(`rejects signed invalid envelope: ${name}`, () => {
    assert.equal(verifyWikiSnapshotV1(fixture(change)), null);
  });

test("shared Rust and TypeScript fixture preserves canonical signed bytes", () => {
  const golden = JSON.parse(
    readFileSync(
      new URL(
        "../../../../../crates/crew-wiki/tests/fixtures/wiki-snapshot-v1.json",
        import.meta.url,
      ),
      "utf8",
    ),
  );
  assert.ok(verifyWikiSnapshotV1(golden));
  const fresh = clone(fixture());
  assert.ok(verifyWikiSnapshotV1(fresh));
  for (const [actual, expected] of [
    [fresh.head, golden.head],
    [fresh.manifest, golden.manifest],
    [fresh.pages[0], golden.pages[0]],
  ]) {
    // Schnorr auxiliary randomness changes the signature, not canonical signed data.
    const { sig: _actualSig, ...actualData } = actual;
    const { sig: _expectedSig, ...expectedData } = expected;
    assert.deepEqual(actualData, expectedData);
  }
});

test("rejects a validly signed body whose immutable envelope digest is stale", () => {
  const input = clone(fixture());
  input.pages[0].content += "Signed but not content addressed correctly.";
  input.pages[0] = resign(input.pages[0]);
  repoint(input);
  assert.equal(verifyEvent(input.pages[0]), true);
  assert.equal(verifyEvent(input.manifest), true);
  assert.equal(verifyEvent(input.head), true);
  assert.equal(verifyWikiSnapshotV1(input), null);
});

test("rejects a page signed by another author despite owner tags and exact membership", () => {
  const input = clone(fixture());
  const page = input.pages[0];
  input.pages[0] = finalizeEvent(
    {
      kind: page.kind,
      created_at: page.created_at,
      content: page.content,
      tags: page.tags,
    },
    new Uint8Array(32).fill(2),
  );
  repoint(input);
  assert.equal(verifyEvent(input.pages[0]), true);
  assert.equal(verifyWikiSnapshotV1(input), null);
});

for (const [name, mutate] of [
  [
    "manifest reference id",
    (x) => {
      x.head.tags.find((t) => t[0] === "wiki-manifest")[1] = "c".repeat(64);
      x.head = resign(x.head);
    },
  ],
  [
    "manifest reference digest",
    (x) => {
      x.head.tags.find((t) => t[0] === "wiki-manifest")[2] = "c".repeat(64);
      x.head = resign(x.head);
    },
  ],
  [
    "missing expected revision",
    (x) => {
      x.head.tags = x.head.tags.filter((t) => t[0] !== "expected-revision");
      x.head = resign(x.head);
    },
  ],
  [
    "invalid expected revision",
    (x) => {
      x.head.tags.find((t) => t[0] === "expected-revision")[1] = "missing";
      x.head = resign(x.head);
    },
  ],
  [
    "page title tag mismatch",
    (x) => {
      x.pages[0].tags.find((t) => t[0] === "title")[1] = "Other";
      x.pages[0] = resign(x.pages[0]);
      repoint(x);
    },
  ],
  [
    "legacy source projection mismatch",
    (x) => {
      x.pages[0].tags = x.pages[0].tags.filter((t) => t[0] !== "source");
      x.pages[0] = resign(x.pages[0]);
      repoint(x);
    },
  ],
  [
    "noncanonical manifest whitespace",
    (x) =>
      rewriteIndex(
        x,
        (b) => b,
        (b) => JSON.stringify(b, null, 2),
      ),
  ],
  [
    "section membership mismatch",
    (x) =>
      rewriteIndex(x, (b) => {
        b[7][0][5] = "other";
        return b;
      }),
  ],
  [
    "duplicate section id",
    (x) =>
      rewriteIndex(x, (b) => {
        b[6].push(["overview", "Duplicate", []]);
        return b;
      }),
  ],
  [
    "duplicate section slug",
    (x) =>
      rewriteIndex(x, (b) => {
        b[6][0][2].push("intro");
        return b;
      }),
  ],
])
  test(`rejects otherwise signed guard case: ${name}`, () => {
    const x = clone(fixture());
    mutate(x);
    assert.equal(verifyWikiSnapshotV1(x), null);
  });

for (const [name, refs] of [
  [
    "out of order paths",
    [
      ["z.rs", "b".repeat(64), 7, 1, 1],
      ["a.rs", "b".repeat(64), 7, 1, 1],
    ],
  ],
  [
    "duplicate range",
    [
      ["a.rs", "b".repeat(64), 7, 1, 1],
      ["a.rs", "b".repeat(64), 7, 1, 1],
    ],
  ],
  [
    "same path conflicting hash",
    [
      ["a.rs", "b".repeat(64), 7, 1, 1],
      ["a.rs", "c".repeat(64), 7, 2, 2],
    ],
  ],
  [
    "same path conflicting length",
    [
      ["a.rs", "b".repeat(64), 7, 1, 1],
      ["a.rs", "b".repeat(64), 8, 2, 2],
    ],
  ],
])
  test(`rejects signed source refs: ${name}`, () =>
    assert.equal(
      verifyWikiSnapshotV1(
        fixture((e) => {
          e[9] = refs;
          return e;
        }),
      ),
      null,
    ));

test("folder snapshot accepts typed revision but rejects a branch projection", () => {
  const x = clone(
    fixture((e) => {
      e[4] = `folder:${"c".repeat(64)}`;
      return e;
    }),
  );
  assert.ok(verifyWikiSnapshotV1(x));
  x.head.tags.push(["branch", "main"]);
  x.head = resign(x.head);
  assert.equal(verifyWikiSnapshotV1(x), null);
});

test("complete signed event cap rejects an oversized valid body", () => {
  assert.equal(
    verifyWikiSnapshotV1(
      fixture((e) => {
        e[10] = "x".repeat(192 * 1024);
        return e;
      }),
    ),
    null,
  );
});

test("metadata emptiness uses the same explicit ASCII set in each language", () => {
  for (const title of ["\u0085", "\ufeff"]) {
    assert.ok(
      verifyWikiSnapshotV1(
        fixture((e) => {
          e[6] = title;
          return e;
        }),
      ),
    );
  }
  assert.equal(
    verifyWikiSnapshotV1(
      fixture((e) => {
        e[6] = " \t\r\n";
        return e;
      }),
    ),
    null,
  );
});

test("shared escaping fixture covers JSON controls and UTF-8 path ordering", () => {
  const input = JSON.parse(
    readFileSync(
      new URL(
        "../../../../../crates/crew-wiki/tests/fixtures/wiki-snapshot-v1-escaping.json",
        import.meta.url,
      ),
      "utf8",
    ),
  );
  const verified = verifyWikiSnapshotV1(input);
  assert.ok(verified);
  assert.deepEqual(
    verified.manifest[7][0][7].map((ref) => ref[0]),
    ["\ue000.rs", "\u{10000}.rs"],
  );
  for (const codepoint of [
    '"',
    "\\",
    "\b",
    "\f",
    "\n",
    "\r",
    "\t",
    "\u001f",
    "\u007f",
    "\u2028",
    "\u2029",
    "/",
    "😀",
  ]) {
    assert.ok(verified.pages[0].content.includes(codepoint));
  }
});

test("literal negative-zero wire syntax is rejected by canonical reserialization", () => {
  const input = clone(fixture());
  rewriteIndex(
    input,
    (body) => body,
    (body) => JSON.stringify(body).replace(",7,1,1]", ",7,-0,1]"),
  );
  assert.equal(verifyWikiSnapshotV1(input), null);
});

test("equivalent exponent syntax still fails canonical manifest byte comparison", () => {
  const input = clone(fixture());
  rewriteIndex(
    input,
    (body) => body,
    (body) => JSON.stringify(body).replace(",7,1,1]", ",7e0,1,1]"),
  );
  assert.equal(verifyWikiSnapshotV1(input), null);
});
