import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { splitEvidenceBody } from "./evidenceBodyParts.ts";

describe("splitEvidenceBody", () => {
  it("extracts Tests/Diff claim lines and promotes PR links", () => {
    const parts = splitEvidenceBody(
      [
        "@Oscar PR is up:",
        "",
        "- GitHub: https://github.com/Nuncio-hq/crew/pull/267",
        "- Buzz PR: buzz://pr?id=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa&owner=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb&d=anunciocrew",
        "",
        "Tests: 1127 passed, 0 failed",
        "Diff: +215/−1 across 5 files",
        "",
        "Timeout gap is closed.",
      ].join("\n"),
    );

    assert.deepEqual(parts.claimLines, [
      "Tests: 1127 passed, 0 failed",
      "Diff: +215/−1 across 5 files",
    ]);
    assert.equal(parts.links.length, 2);
    assert.equal(parts.links[0]?.kind, "github-pr");
    assert.equal(parts.links[0]?.label, "Open PR on GitHub");
    assert.equal(parts.links[1]?.kind, "buzz-pr");
    assert.equal(parts.links[1]?.label, "Open PR in Crew");
    assert.match(parts.narrative, /@Oscar PR is up/);
    assert.match(parts.narrative, /Timeout gap is closed/);
    assert.doesNotMatch(parts.narrative, /github\.com/);
    assert.doesNotMatch(parts.narrative, /buzz:\/\//);
    assert.doesNotMatch(parts.narrative, /Tests:/);
  });

  it("keeps empty/malformed input safe", () => {
    assert.deepEqual(splitEvidenceBody(""), {
      claimLines: [],
      links: [],
      narrative: "",
    });
  });
});
