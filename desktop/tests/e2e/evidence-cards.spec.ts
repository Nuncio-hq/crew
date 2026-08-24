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
  mock: Parameters<typeof installMockBridge>[1] = {},
) {
  await installMockBridge(page, {
    ...mock,
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
    content:
      "Local suite finished.\nTests: 1 failed → 1 passed\n\nFailed:\n- login rejects bad password\n\nPassed:\n- login accepts owner\n\nhttps://github.com/Nuncio-hq/crew/pull/9",
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
    "before-after-visual",
    "diff-stat",
  ]) {
    const card = page.getByTestId(`evidence-card-${kind}`);
    await expect(card).toBeVisible();
    await card.screenshot({ path: `test-results/evidence-cards/${kind}.png` });
  }

  const testRun = page.getByTestId("evidence-card-test-run");
  await expect(testRun.getByTestId("test-run-summary")).toBeVisible();
  await expect(testRun.getByTestId("test-run-summary")).toContainText(
    "1 passed",
  );
  await expect(testRun.getByTestId("test-run-summary")).toContainText(
    "1 failed",
  );
  await expect(testRun).toContainText("Local suite finished");
  await expect(testRun.getByTestId("evidence-link-github-pr")).toContainText(
    "Open PR on GitHub",
  );
  await expect(testRun).not.toContainText("Failing");
  await expect(testRun).toContainText("Test run");

  // Expand like Cursor Checks — show named rows, not just the count line.
  await testRun.getByTestId("test-run-summary-toggle").click();
  const details = testRun.getByTestId("test-run-summary-details");
  await expect(details).toBeVisible();
  await expect(details).toContainText("login rejects bad password");
  await expect(details).toContainText("login accepts owner");
  await testRun.screenshot({
    path: "test-results/evidence-cards/test-run-expanded.png",
  });

  await expect(page.getByTestId("evidence-card-metrics")).toContainText(
    "before",
  );
  await expect(page.getByTestId("evidence-card-metrics")).toContainText(
    "120ms",
  );

  const diff = page.getByTestId("evidence-card-diff-stat");
  await expect(diff.getByTestId("diff-stat-summary")).toContainText("+42");
  await expect(diff.getByTestId("diff-stat-summary")).toContainText("−17");
  await expect(diff.getByTestId("diff-stat-summary")).toContainText("4 files");
});

test("evidence cards preserve and render Markdown images", async ({ page }) => {
  const sha256 = "a".repeat(64);
  const sourceUrl = `http://localhost:3000/media/${sha256}.png`;
  const proxyUrl = `http://127.0.0.1:54321/media/${sha256}.png`;

  await page.route(proxyUrl, (route) =>
    route.fulfill({
      body: '<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20" fill="#22c55e"/></svg>',
      contentType: "image/svg+xml",
    }),
  );
  await openEvidenceChannel(page);
  await emit(page, {
    content: `Proxy readiness evidence\n\n![image](${sourceUrl})`,
    extraTags: [
      ...EVIDENCE_TAG("test-run"),
      ["imeta", `url ${sourceUrl}`, "m image/png", `x ${sha256}`, "dim 40x20"],
    ],
  });

  const card = page.getByTestId("evidence-card-test-run");
  const trigger = card.getByTestId("message-image-lightbox-trigger");
  const image = trigger.locator("img").last();
  await expect(trigger).toHaveAttribute(
    "data-image-lightbox-resolved-src",
    proxyUrl,
  );
  await expect(card.getByTestId("evidence-links")).toHaveCount(0);
  await expect.poll(() => image.evaluate((node) => node.naturalWidth)).toBe(40);
});

test("evidence cards preserve protected videos for inline proxy playback", async ({
  page,
}) => {
  const sha256 = "d".repeat(64);
  const sourceUrl = `http://localhost:3000/media/${sha256}.mp4`;
  const proxyUrl = `http://127.0.0.1:54321/media/${sha256}.mp4`;

  await openEvidenceChannel(page);
  await emit(page, {
    content: `Recorded evidence\n\n![video](${sourceUrl})`,
    extraTags: [
      ...EVIDENCE_TAG("test-run"),
      [
        "imeta",
        `url ${sourceUrl}`,
        "m video/mp4",
        `x ${sha256}`,
        "dim 160x90",
        "duration 6.8",
        "filename evidence.mp4",
      ],
    ],
  });

  const card = page.getByTestId("evidence-card-test-run");
  const player = card.getByTestId("video-player");
  await expect(player).toBeVisible();
  await expect(player.locator("video")).toHaveAttribute("src", proxyUrl);
  await expect(player.getByTestId("video-inline-duration")).toHaveText("00:06");
  await expect(card.getByTestId("evidence-links")).toHaveCount(0);
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
