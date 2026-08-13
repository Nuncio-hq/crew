import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { parseForgePullRequestUrl, parseRepoAddress } from "./parseForgePullRequestUrl.ts";
import { parseCrewFinding } from "./parseCrewFinding.ts";
import { resolveReviewerFromCanvas } from "./resolveReviewerFromCanvas.ts";

describe("parseForgePullRequestUrl", () => {
  it("parses github pull URLs and hash locators", () => {
    assert.deepEqual(
      parseForgePullRequestUrl("https://github.com/Nuncio-hq/crew/pull/193"),
      { owner: "Nuncio-hq", name: "crew", number: 193 },
    );
    assert.deepEqual(parseForgePullRequestUrl("Nuncio-hq/crew#12"), {
      owner: "Nuncio-hq",
      name: "crew",
      number: 12,
    });
    assert.equal(parseForgePullRequestUrl("https://example.com/nope"), null);
    assert.deepEqual(parseRepoAddress("Nuncio-hq/crew"), {
      owner: "Nuncio-hq",
      name: "crew",
    });
  });
});

describe("parseCrewFinding", () => {
  it("reads a tolerant crew-finding tag", () => {
    assert.deepEqual(
      parseCrewFinding([
        ["p", "aa"],
        ["crew-finding", "error", "src/foo.rs", "12-14"],
      ]),
      { severity: "error", file: "src/foo.rs", range: "12-14" },
    );
    assert.deepEqual(parseCrewFinding([["crew-finding", "weird"]]), {
      severity: "info",
      file: null,
      range: null,
    });
    assert.equal(parseCrewFinding([["crew-evidence", "test-run"]]), null);
  });
});

describe("resolveReviewerFromCanvas", () => {
  const holder = "22".repeat(32);
  const routingHit = {
    routing: [
      {
        workType: "code-review",
        roleLabel: "Reviewer",
        holders: [holder],
        unheldMessage: null,
      },
    ],
    assignments: [],
  };

  it("prefers code-review routing holders", () => {
    const resolved = resolveReviewerFromCanvas(routingHit);
    assert.equal(resolved.status, "held");
    if (resolved.status === "held") {
      assert.equal(resolved.pubkey, holder);
      assert.equal(resolved.source, "routing");
    }
  });

  it("falls back to a Reviewer role label", () => {
    const resolved = resolveReviewerFromCanvas({
      routing: [],
      assignments: [{ agentPubkey: holder, roleLabel: "Reviewer" }],
    });
    assert.equal(resolved.status, "held");
    if (resolved.status === "held") {
      assert.equal(resolved.source, "label");
    }
  });

  it("is unheld when nobody holds the role", () => {
    const resolved = resolveReviewerFromCanvas({
      routing: [
        {
          workType: "code-review",
          roleLabel: "Reviewer",
          holders: [],
          unheldMessage: "ask the founder",
        },
      ],
      assignments: [],
    });
    assert.equal(resolved.status, "unheld");
  });

  it("ignores empty canvas the same as a non-owner canvas", () => {
    const resolved = resolveReviewerFromCanvas({
      routing: [],
      assignments: [],
    });
    assert.equal(resolved.status, "unheld");
  });

  it("uses a thread override before routing", () => {
    const resolved = resolveReviewerFromCanvas(routingHit, {
      threadPubkey: "ab".repeat(32),
    });
    assert.equal(resolved.status, "held");
    if (resolved.status === "held") {
      assert.equal(resolved.source, "manual");
      assert.equal(resolved.pubkey, "ab".repeat(32));
    }
  });
});
