import { expect, test } from "@playwright/test";

import { installMockBridge, TEST_IDENTITIES } from "../helpers/bridge";

const CHANNEL = "general";
const EVIDENCE_TAG = (kind: string) => [["crew-evidence", kind]];
const RECEIPT_BODY = JSON.stringify({
  summary: "Evidence receipt",
  verify: "Review the attached evidence.",
  lights: [{ label: "tests", status: "pass" }],
  engineering: {
    pr_ref: null,
    branch: "evidence",
    files_changed: ["README.md"],
    ci: [],
  },
});

async function openEvidenceChannel(
  page: import("@playwright/test").Page,
) {
  await installMockBridge(page, {
    searchProfiles: [
      {
        pubkey: TEST_IDENTITIES.alice.pubkey,
        displayName: "Evidence Agent",
        ownerPubkey: TEST_IDENTITIES.tyler.pubkey,
        isAgent: true,
      },
    ],
  });
  await page.goto("/");
  await page.getByTestId("channel-general").click();
  await expect(page.getByTestId("chat-title")).toHaveText(CHANNEL);
  await page.waitForFunction(
    () => typeof window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__ === "function",
  );
}

async function emit(
  page: import("@playwright/test").Page,
  input: {
    content: string;
    kind?: number;
    pubkey?: string;
    extraTags?: string[][];
  },
) {
  return page.evaluate(
    ({ channelName, content, kind, pubkey, extraTags }) =>
      window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__?.({
        channelName,
        content,
        kind,
        pubkey,
        extraTags,
      }),
    { ...input, channelName: CHANNEL },
  );
}

test("each evidence kind renders a legible card", async ({ page }) => {
  await openEvidenceChannel(page);
  await emit(page, {
    content: "Tests: 1 failed → 1 passed",
    extraTags: EVIDENCE_TAG("test-run"),
  });
  await emit(page, {
    content: "before: 120ms | after: 80ms | delta: -40ms",
    extraTags: EVIDENCE_TAG("metrics"),
  });
  await emit(page, {
    content: "Files: 4 | +42 −17",
    extraTags: EVIDENCE_TAG("diff-stat"),
  });
  await emit(page, {
    content: "before screenshot\n\nafter screenshot",
    extraTags: EVIDENCE_TAG("before-after-visual"),
  });

  for (const kind of [
    "test-run",
    "metrics",
    "diff-stat",
    "before-after-visual",
  ]) {
    const card = page.getByTestId(`evidence-card-${kind}`);
    await expect(card).toBeVisible();
    await card.screenshot({ path: `test-results/evidence-cards/${kind}.png` });
  }
  await expect(page.getByTestId("evidence-card-test-run")).toContainText(
    "1 passed",
  );
  await expect(page.getByTestId("evidence-card-metrics")).toContainText(
    "delta",
  );
  await expect(page.getByTestId("evidence-card-diff-stat")).toContainText(
    "+42",
  );
});

test("ordinary and unrecognized evidence messages keep ordinary rendering", async ({
  page,
}) => {
  await openEvidenceChannel(page);
  await emit(page, { content: "ordinary evidence body" });
  await emit(page, {
    content: "unknown evidence body",
    extraTags: EVIDENCE_TAG("future-kind"),
  });

  await expect(page.getByText("ordinary evidence body")).toBeVisible();
  await expect(page.getByText("unknown evidence body")).toBeVisible();
  await expect(page.locator('[data-testid^="evidence-card-"]')).toHaveCount(0);
});

test("agent receipt with evidence tag keeps one receipt card", async ({
  page,
}) => {
  await openEvidenceChannel(page);
  await emit(page, {
    content: RECEIPT_BODY,
    kind: 46043,
    pubkey: TEST_IDENTITIES.alice.pubkey,
    extraTags: EVIDENCE_TAG("test-run"),
  });

  await expect(page.getByTestId("agent-receipt-card")).toBeVisible();
  await expect(page.locator('[data-testid^="evidence-card-"]')).toHaveCount(0);
});
