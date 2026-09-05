/**
 * Mounted-provider regression guard for upstream #5825 (Crew issue #275).
 *
 * The Rust playout loop emits `huddle-speaker-levels` every 50 ms for the whole
 * life of a huddle. When those levels lived on the single huddle context value,
 * every tick minted a new context identity and re-rendered every
 * `useHuddle()` consumer — including `ChannelScreen` and every message row.
 *
 * These tests mount the real `HuddleProvider` over a stubbed Tauri IPC surface
 * and count renders of both consumer kinds while driving level ticks:
 *
 *   - a `useHuddle()` consumer must not re-render on level churn;
 *   - a `useHuddleLevels()` consumer must still see every tick.
 */

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  emitTauriEvent,
  mountProviderWithProbes,
  SPEAKER_PUBKEY,
} from "./huddle-provider-fixture.mjs";

test("speaker-level ticks do not re-render useHuddle consumers", async () => {
  const { act, cleanup, renders } = await mountProviderWithProbes();
  try {
    const baseline = renders.huddle;
    const levelsBaseline = renders.levels;

    for (let tick = 0; tick < 20; tick += 1) {
      await act(async () => {
        emitTauriEvent("huddle-speaker-levels", {
          [SPEAKER_PUBKEY]: tick / 20,
        });
      });
    }

    assert.equal(
      renders.huddle,
      baseline,
      "useHuddle consumers must be insulated from 20 Hz level churn",
    );
    assert.ok(
      renders.levels >= levelsBaseline + 20,
      `level meters must still see every tick (saw ${renders.levels - levelsBaseline})`,
    );
  } finally {
    cleanup();
  }
});

test("active-speaker ticks do not re-render useHuddle consumers", async () => {
  const { act, cleanup, renders } = await mountProviderWithProbes();
  try {
    const baseline = renders.huddle;

    for (let tick = 0; tick < 5; tick += 1) {
      await act(async () => {
        emitTauriEvent("huddle-active-speakers", [`speaker-${tick}`]);
      });
    }

    assert.equal(renders.huddle, baseline);
  } finally {
    cleanup();
  }
});
