import { expect, type Locator, type Page } from "@playwright/test";

const LETTER_SOUP_MIN_CH = 6;

export type AssertPaneResponsiveOptions = {
  /** Pairs of test ids that must not share overlapping bounding boxes. */
  mustNotOverlap?: ReadonlyArray<readonly [string, string]>;
};

function rectsOverlap(
  a: { bottom: number; left: number; right: number; top: number },
  b: { bottom: number; left: number; right: number; top: number },
): boolean {
  return !(
    a.right <= b.left + 0.5 ||
    b.right <= a.left + 0.5 ||
    a.bottom <= b.top + 0.5 ||
    b.bottom <= a.top + 0.5
  );
}

/**
 * Overflow / letter-soup / chrome-overlap checks for one pane (#205).
 * Future specs: `await assertPaneResponsive(page, "message-thread-panel")`.
 */
export async function assertPaneResponsive(
  page: Page,
  paneTestId: string,
  options: AssertPaneResponsiveOptions = {},
): Promise<void> {
  const pane = page.getByTestId(paneTestId);
  await expect(pane).toBeVisible();

  const report = await pane.evaluate((element, minCh) => {
    const paneRect = element.getBoundingClientRect();
    const overflow: string[] = [];
    if (element.scrollWidth > element.clientWidth + 1) {
      overflow.push(
        `${element.getAttribute("data-testid") ?? element.tagName} scrollWidth ${element.scrollWidth} > clientWidth ${element.clientWidth}`,
      );
    }

    const soup: string[] = [];
    const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
    let node = walker.nextNode();
    while (node) {
      const text = node.textContent?.replace(/\s+/g, "") ?? "";
      const parent = node.parentElement;
      node = walker.nextNode();
      if (!parent || text.length < 4) continue;
      const style = getComputedStyle(parent);
      if (style.visibility === "hidden" || style.display === "none") {
        continue;
      }
      if (
        parent.closest(
          "[aria-hidden='true'], .sr-only, [data-radix-collection-item]",
        )
      ) {
        continue;
      }
      if (
        style.textOverflow === "ellipsis" ||
        style.overflow === "hidden" ||
        style.overflowX === "auto" ||
        style.overflowX === "scroll"
      ) {
        continue;
      }
      const rect = parent.getBoundingClientRect();
      if (rect.width <= 0 || rect.height <= 0) continue;
      const fontSize = Number.parseFloat(style.fontSize) || 16;
      const ch = fontSize * 0.5;
      const minWidth = minCh * ch;
      const lineHeight = Number.parseFloat(style.lineHeight) || fontSize * 1.2;
      if (rect.width + 0.5 < minWidth && rect.height > lineHeight * 2.2) {
        soup.push(
          `"${text.slice(0, 24)}" ${Math.round(rect.width)}×${Math.round(rect.height)}px`,
        );
      }
      if (rect.right > paneRect.right + 2 && rect.width > 12) {
        const overflowing = parent.closest(
          ".overflow-x-auto, [class*='overflow-x-auto']",
        );
        if (!overflowing) {
          overflow.push(
            `${parent.tagName} extends ${Math.round(rect.right - paneRect.right)}px past pane`,
          );
        }
      }
    }

    return { overflow, soup };
  }, LETTER_SOUP_MIN_CH);

  expect(report.overflow, report.overflow.join("; ")).toEqual([]);
  expect(report.soup, report.soup.join("; ")).toEqual([]);

  for (const [leftId, rightId] of options.mustNotOverlap ?? []) {
    const left = page.getByTestId(leftId);
    const right = page.getByTestId(rightId);
    if ((await left.count()) === 0 || (await right.count()) === 0) continue;
    if (!(await left.isVisible()) || !(await right.isVisible())) continue;
    const leftBox = await left.boundingBox();
    const rightBox = await right.boundingBox();
    if (!leftBox || !rightBox) continue;
    expect(
      rectsOverlap(
        {
          left: leftBox.x,
          top: leftBox.y,
          right: leftBox.x + leftBox.width,
          bottom: leftBox.y + leftBox.height,
        },
        {
          left: rightBox.x,
          top: rightBox.y,
          right: rightBox.x + rightBox.width,
          bottom: rightBox.y + rightBox.height,
        },
      ),
      `${leftId} overlaps ${rightId}`,
    ).toBe(false);
  }
}

export async function assertOverlayInsideWindow(
  overlay: Locator,
  page: Page,
): Promise<void> {
  await expect(overlay).toBeVisible();
  const box = await overlay.boundingBox();
  const viewport = page.viewportSize();
  expect(box, "overlay bounding box").toBeTruthy();
  expect(viewport, "viewport").toBeTruthy();
  if (!box || !viewport) return;
  expect(box.x).toBeGreaterThanOrEqual(-1);
  expect(box.y).toBeGreaterThanOrEqual(-1);
  expect(box.x + box.width).toBeLessThanOrEqual(viewport.width + 1);
  expect(box.y + box.height).toBeLessThanOrEqual(viewport.height + 1);
}
