import { expect, type Locator, type Page } from "@playwright/test";

type MentionExpectation = {
  eventId: string;
  renderedContent: string;
  label: string;
  pubkey: string;
};

export async function expectMentionMessage(
  scope: Locator,
  mention: MentionExpectation,
) {
  expect(mention.eventId).toMatch(/^[a-f0-9]{64}$/);
  const row = scope.locator(`[data-message-id="${mention.eventId}"]`);
  await expect(row).toHaveCount(1);
  const body = row.getByTestId("message-body");
  await expect(body).toHaveText(mention.renderedContent);
  const chip = body.locator("[data-mention]");
  await expect(chip).toHaveCount(1);
  await expect(chip).toHaveText(mention.label);
  await expect(chip).toHaveAttribute("data-mention-label", mention.label);
  await expect(chip).toHaveAttribute("data-mention-pubkey", mention.pubkey);
}

export async function expectInboxMention(
  page: Page,
  mention: MentionExpectation,
) {
  const item = page.getByTestId(`home-inbox-item-${mention.eventId}`);
  await expect(item).toHaveCount(1);
  const preview = item.locator(".inbox-preview-markdown");
  await expect(preview).toHaveText(mention.renderedContent);
  const chip = preview.locator("[data-mention]");
  await expect(chip).toHaveCount(1);
  await expect(chip).toHaveText(mention.label);
  await expect(chip).toHaveAttribute("data-mention-label", mention.label);
  // The preview exposes a label only. The automatically selected detail
  // exposes the recipient key; bind it to the SAME acknowledged event without
  // clicking/navigating or requesting a refetch to manufacture delivery.
  await expectMentionMessage(page.getByTestId("home-inbox-detail"), mention);
}
