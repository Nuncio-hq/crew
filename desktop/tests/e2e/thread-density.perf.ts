import { expect, test } from "@playwright/test";
import type { CDPSession, Page } from "@playwright/test";

import { installMockBridge } from "../helpers/bridge";

/**
 * Measures whether main-timeline scrolling and warm thread opening scale with
 * the number of thread roots in one channel. The channel timeline is already
 * virtualized, so mounted DOM rows should remain bounded; this harness catches
 * CPU/data derivations that still scale with the full loaded channel window.
 *
 * Run from desktop/ after `pnpm build:e2e`:
 *   pnpm exec playwright test --config=playwright.perf.config.ts \
 *     thread-density.perf.ts
 */

const THROTTLE_RATE = 4;
const OPEN_RUNS = 5;
const SCENARIOS = [
  { channelName: "engineering", rootCount: 40 },
  { channelName: "random", rootCount: 140 },
] as const;

type BrowserMetrics = {
  layoutMs: number;
  recalcMs: number;
  scriptMs: number;
  taskMs: number;
};

type OpenSample = {
  ms: number;
  longtaskMs: number;
  longestLongtaskMs: number;
};

type ScenarioResult = {
  channelName: string;
  rootCount: number;
  loadedPageCount: number;
  mountedThreadSummaryCount: number;
  mountedMessageCount: number;
  openMedianMs: number;
  openP95Ms: number;
  openLongtaskMedianMs: number;
  scrollLayoutMs: number;
  scrollRecalcMs: number;
  scrollScriptMs: number;
  scrollTaskMs: number;
  scrollFrames: number;
  scrollSpanPx: number;
  settledMountedMessageCount: number;
  settledScrollLayoutMs: number;
  settledScrollRecalcMs: number;
  settledScrollScriptMs: number;
  settledScrollTaskMs: number;
};

function percentile(values: number[], quantile: number): number {
  const sorted = [...values].sort((left, right) => left - right);
  if (sorted.length === 0) return 0;
  return sorted[
    Math.min(sorted.length - 1, Math.floor(quantile * sorted.length))
  ];
}

async function readMetrics(client: CDPSession): Promise<BrowserMetrics> {
  const { metrics } = (await client.send("Performance.getMetrics")) as {
    metrics: Array<{ name: string; value: number }>;
  };
  const value = (name: string) =>
    metrics.find((metric) => metric.name === name)?.value ?? 0;
  return {
    layoutMs: value("LayoutDuration") * 1_000,
    recalcMs: value("RecalcStyleDuration") * 1_000,
    scriptMs: value("ScriptDuration") * 1_000,
    taskMs: value("TaskDuration") * 1_000,
  };
}

async function seedThreadRoots(
  page: Page,
  channelName: string,
  rootCount: number,
): Promise<string[]> {
  return page.evaluate(
    ({ channel, count }) => {
      const emit = window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__;
      if (!emit) throw new Error("Mock message emitter is not installed.");
      const base = Math.floor(Date.now() / 1_000) - count * 3 - 30;
      const rootIds: string[] = [];
      for (let index = 0; index < count; index += 1) {
        const root = emit({
          channelName: channel,
          content: `Thread ${index}: coordinate release work`,
          createdAt: base + index * 2,
          emitLive: false,
        });
        rootIds.push(root.id);
        emit({
          channelName: channel,
          content: `Reply ${index}: acknowledged`,
          parentEventId: root.id,
          createdAt: base + index * 2 + 1,
          emitLive: false,
        });
      }
      return rootIds;
    },
    { channel: channelName, count: rootCount },
  );
}

async function waitForTimelineSettled(page: Page) {
  const timeline = page.getByTestId("message-timeline");
  await expect(timeline.locator("[data-message-id]").first()).toBeVisible();
  await expect(page.locator('[data-render-pending="true"]')).toHaveCount(0, {
    timeout: 30_000,
  });
  await page.evaluate(
    () =>
      new Promise<void>((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
      ),
  );
}

async function loadAllThreadRoots(page: Page, rootCount: number) {
  const timeline = page.getByTestId("message-timeline");
  const expectedPageCount = Math.ceil(rootCount / 50);
  const activeChannelId = await page.evaluate(() => {
    const latest = (window.__BUZZ_E2E_COMMAND_LOG__ ?? [])
      .filter((entry) => entry.command === "get_channel_window")
      .at(-1);
    const channelId = (latest?.payload as { channelId?: unknown } | undefined)
      ?.channelId;
    if (typeof channelId !== "string") {
      throw new Error("Active channel window command was not logged.");
    }
    return channelId;
  });
  const olderChannelWindowCommandCount = () =>
    page.evaluate(
      (channelId) =>
        (window.__BUZZ_E2E_COMMAND_LOG__ ?? []).filter(
          (entry) =>
            entry.command === "get_channel_window" &&
            (entry.payload as { channelId?: unknown }).channelId ===
              channelId &&
            (entry.payload as { cursor?: unknown }).cursor != null,
        ).length,
      activeChannelId,
    );
  let commandCount = await olderChannelWindowCommandCount();
  while (commandCount < expectedPageCount - 1) {
    const previousCommandCount = commandCount;
    for (let attempt = 0; attempt < 50; attempt += 1) {
      if ((await olderChannelWindowCommandCount()) > previousCommandCount)
        break;
      await timeline.evaluate((element) => {
        const scroller = element as HTMLDivElement;
        scroller.dispatchEvent(
          new WheelEvent("wheel", { deltaY: 1_500, bubbles: true }),
        );
        scroller.scrollTop = Math.min(scroller.scrollHeight, 1_500);
        scroller.dispatchEvent(new Event("scroll", { bubbles: true }));
      });
      await page.waitForTimeout(50);
      await timeline.evaluate((element) => {
        const scroller = element as HTMLDivElement;
        scroller.dispatchEvent(
          new WheelEvent("wheel", { deltaY: -1, bubbles: true }),
        );
        scroller.scrollTop = 0;
        scroller.dispatchEvent(new Event("scroll", { bubbles: true }));
      });
      await page.waitForTimeout(50);
    }
    const nextCommandCount = await olderChannelWindowCommandCount();
    if (nextCommandCount <= previousCommandCount) {
      const payloads = await page.evaluate(
        (channelId) =>
          (window.__BUZZ_E2E_COMMAND_LOG__ ?? [])
            .filter(
              (entry) =>
                entry.command === "get_channel_window" &&
                (entry.payload as { channelId?: unknown }).channelId ===
                  channelId,
            )
            .map((entry) => entry.payload),
        activeChannelId,
      );
      throw new Error(
        `Older channel window did not load: ${JSON.stringify(payloads)}`,
      );
    }
    commandCount = await olderChannelWindowCommandCount();
    await page.waitForTimeout(100);
  }

  await timeline.evaluate((element) => {
    const scroller = element as HTMLDivElement;
    scroller.scrollTop = scroller.scrollHeight;
    scroller.dispatchEvent(new Event("scroll", { bubbles: true }));
  });
  await page.waitForTimeout(200);
  return expectedPageCount;
}

async function setThreadOpen(page: Page, rootId: string, open: boolean) {
  const summary = page.locator(`[data-thread-head-id="${rootId}"]`);
  await expect(summary).toBeVisible({ timeout: 30_000 });
  await summary.click();
  const body = page.getByTestId("message-thread-body");
  if (open) {
    await expect(body).toBeVisible({ timeout: 30_000 });
  } else {
    await expect(body).toHaveCount(0, { timeout: 30_000 });
  }
}

async function measureWarmOpen(
  page: Page,
  rootId: string,
): Promise<OpenSample> {
  return page.evaluate(async (id) => {
    const store = window as unknown as {
      __THREAD_DENSITY_LONGTASKS__: number[];
    };
    store.__THREAD_DENSITY_LONGTASKS__ = [];
    const summary = document.querySelector<HTMLElement>(
      `[data-thread-head-id="${CSS.escape(id)}"]`,
    );
    if (!summary) throw new Error(`Thread summary ${id} is not mounted.`);
    const start = performance.now();
    summary.click();
    await new Promise<void>((resolve, reject) => {
      const deadline = start + 30_000;
      const check = () => {
        const ready =
          document.querySelector('[data-testid="message-thread-body"]') !==
          null;
        if (ready) {
          requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
          return;
        }
        if (performance.now() > deadline) {
          reject(new Error("Warm thread open timed out."));
          return;
        }
        requestAnimationFrame(check);
      };
      requestAnimationFrame(check);
    });
    const tasks = store.__THREAD_DENSITY_LONGTASKS__ ?? [];
    return {
      ms: performance.now() - start,
      longtaskMs: tasks.reduce((sum, duration) => sum + duration, 0),
      longestLongtaskMs: tasks.length ? Math.max(...tasks) : 0,
    };
  }, rootId);
}

async function measureScroll(page: Page, client: CDPSession) {
  const timeline = page.getByTestId("message-timeline");
  await timeline.evaluate((element) => {
    element.scrollTop = element.scrollHeight;
  });
  await page.waitForTimeout(200);
  const before = await readMetrics(client);
  const sample = await timeline.evaluate(async (element) => {
    const scroller = element as HTMLDivElement;
    const startTop = scroller.scrollTop;
    let frames = 0;
    await new Promise<void>((resolve) => {
      const step = () => {
        frames += 1;
        scroller.scrollTop = Math.max(0, scroller.scrollTop - 160);
        if (frames < 90 && scroller.scrollTop > 0) {
          requestAnimationFrame(step);
        } else {
          resolve();
        }
      };
      requestAnimationFrame(step);
    });
    return { frames, scrollSpanPx: startTop - scroller.scrollTop };
  });
  const after = await readMetrics(client);
  return {
    ...sample,
    layoutMs: after.layoutMs - before.layoutMs,
    recalcMs: after.recalcMs - before.recalcMs,
    scriptMs: after.scriptMs - before.scriptMs,
    taskMs: after.taskMs - before.taskMs,
  };
}

test("MEASURE: channel scroll and warm thread-open cost by thread density", async ({
  page,
}) => {
  test.setTimeout(300_000);
  await page.addInitScript(() => {
    const store = window as unknown as {
      __THREAD_DENSITY_LONGTASKS__?: number[];
    };
    store.__THREAD_DENSITY_LONGTASKS__ = [];
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        store.__THREAD_DENSITY_LONGTASKS__?.push(entry.duration);
      }
    }).observe({ type: "longtask", buffered: true });
  });
  await installMockBridge(page);
  await page.goto("/");
  await page.waitForFunction(
    () => typeof window.__BUZZ_E2E_EMIT_MOCK_MESSAGE__ === "function",
  );

  const rootsByChannel = new Map<string, string[]>();
  for (const scenario of SCENARIOS) {
    rootsByChannel.set(
      scenario.channelName,
      await seedThreadRoots(page, scenario.channelName, scenario.rootCount),
    );
  }

  const client = await page.context().newCDPSession(page);
  await client.send("Performance.enable");
  await client.send("Emulation.setCPUThrottlingRate", { rate: THROTTLE_RATE });
  const results: ScenarioResult[] = [];

  for (const scenario of SCENARIOS) {
    await page.getByTestId(`channel-${scenario.channelName}`).click();
    await expect(page.getByTestId("chat-title")).toHaveText(
      scenario.channelName,
    );
    await waitForTimelineSettled(page);
    const loadedPageCount = await loadAllThreadRoots(page, scenario.rootCount);

    const roots = rootsByChannel.get(scenario.channelName) ?? [];
    const newestRootId = roots.at(-1);
    if (!newestRootId)
      throw new Error(`No roots seeded for ${scenario.channelName}.`);
    await expect(
      page.locator(`[data-thread-head-id="${newestRootId}"]`),
    ).toBeVisible({ timeout: 30_000 });

    const mountedThreadSummaryCount = await page
      .locator('[data-testid="message-timeline"] [data-thread-head-id]')
      .evaluateAll(
        (rows) =>
          new Set(rows.map((row) => row.getAttribute("data-thread-head-id")))
            .size,
      );
    const mountedMessageCount = await page
      .getByTestId("message-timeline")
      .locator("[data-message-id]")
      .count();

    // Warm the reply query and lazy thread-panel chunk before timing UI work.
    await setThreadOpen(page, newestRootId, true);
    await setThreadOpen(page, newestRootId, false);

    const openSamples: OpenSample[] = [];
    for (let run = 0; run < OPEN_RUNS; run += 1) {
      openSamples.push(await measureWarmOpen(page, newestRootId));
      await setThreadOpen(page, newestRootId, false);
    }

    const scroll = await measureScroll(page, client);
    // Every prepend starts a 3s keepMounted eviction guard. Measure again once
    // that guard and the post-scroll retention refresh have both completed to
    // isolate temporary retained-DOM cost from the steady-state virtualizer.
    await page.waitForTimeout(3_500);
    const settledMountedMessageCount = await page
      .getByTestId("message-timeline")
      .locator("[data-message-id]")
      .count();
    const settledScroll = await measureScroll(page, client);
    results.push({
      channelName: scenario.channelName,
      rootCount: scenario.rootCount,
      loadedPageCount,
      mountedThreadSummaryCount,
      mountedMessageCount,
      openMedianMs: percentile(
        openSamples.map((sample) => sample.ms),
        0.5,
      ),
      openP95Ms: percentile(
        openSamples.map((sample) => sample.ms),
        0.95,
      ),
      openLongtaskMedianMs: percentile(
        openSamples.map((sample) => sample.longtaskMs),
        0.5,
      ),
      scrollLayoutMs: scroll.layoutMs,
      scrollRecalcMs: scroll.recalcMs,
      scrollScriptMs: scroll.scriptMs,
      scrollTaskMs: scroll.taskMs,
      scrollFrames: scroll.frames,
      scrollSpanPx: scroll.scrollSpanPx,
      settledMountedMessageCount,
      settledScrollLayoutMs: settledScroll.layoutMs,
      settledScrollRecalcMs: settledScroll.recalcMs,
      settledScrollScriptMs: settledScroll.scriptMs,
      settledScrollTaskMs: settledScroll.taskMs,
    });
  }

  await client.send("Emulation.setCPUThrottlingRate", { rate: 1 });
  await client.send("Performance.disable");

  /* eslint-disable no-console */
  console.log("\n=== THREAD DENSITY PERF (4x CPU) ===");
  console.table(results);
  console.log("====================================\n");
  /* eslint-enable no-console */

  expect(results).toHaveLength(SCENARIOS.length);
  expect(
    results.every((result) => result.settledMountedMessageCount < 120),
  ).toBe(true);
  expect(results.every((result) => result.scrollSpanPx > 500)).toBe(true);
});
