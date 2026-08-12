import assert from "node:assert/strict";
import test from "node:test";

import {
  buildChannelAuxDeletionFilter,
  buildChannelLiveFilter,
  buildChannelAuxFilter,
  buildChannelReactionAuxFilter,
  buildChannelStructuralAuxFilter,
  buildHuddleTtsLiveFilter,
} from "./relayChannelFilters.ts";
import { CHANNEL_LIVE_BACKLOG_GRACE_SECONDS } from "./relayClientTimings.ts";

const CHANNEL = "36411e44-0e2d-4cfe-bd6e-567eb169db9f";
const IDS = [
  "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
];

test("huddle TTS filter includes a bounded startup replay for both message kinds", () => {
  assert.deepEqual(buildHuddleTtsLiveFilter(CHANNEL, 1_725_100_000), {
    kinds: [9, 40002],
    "#h": [CHANNEL],
    since: 1_725_100_000,
    limit: 50,
  });
});

// Regression: reaction (kind:7) and reaction-removal (kind:5) events carry only
// an `e` tag, no channel `h` tag. An `#h`-scoped aux query never matches them,
// so removed historical reactions reappear. The aux filters must key on `#e`
// only.
test("buildChannelAuxFilter keys on #e only, no #h", () => {
  const filter = buildChannelAuxFilter(CHANNEL, IDS);
  assert.deepEqual(filter["#e"], IDS);
  assert.equal("#h" in filter, false);
});

test("buildChannelAuxDeletionFilter keys on #e only, no #h", () => {
  const filter = buildChannelAuxDeletionFilter(CHANNEL, IDS);
  assert.deepEqual(filter["#e"], IDS);
  assert.equal("#h" in filter, false);
});

test("buildChannelReactionAuxFilter fetches only kind:7 by #e", () => {
  const filter = buildChannelReactionAuxFilter(CHANNEL, IDS);
  assert.deepEqual(filter.kinds, [7]);
  assert.deepEqual(filter["#e"], IDS);
  assert.equal("#h" in filter, false);
});

test("buildChannelStructuralAuxFilter excludes reactions", () => {
  const filter = buildChannelStructuralAuxFilter(CHANNEL, IDS);
  assert.deepEqual(filter.kinds, [5, 9005, 40003]);
  assert.deepEqual(filter["#e"], IDS);
  assert.equal("#h" in filter, false);
});

// A `since` of exactly "now" drops an event whose author's clock lags ours and
// one created between the window fetch and this subscription opening; both are
// invisible to the client, so the timeline never recovers them. The grace window
// is the only thing that carries them in — the window store dedups the replay.
test("channel live filter starts a grace window before now", () => {
  const now = 1_725_100_000;
  const filter = buildChannelLiveFilter(CHANNEL, now);
  assert.deepEqual(filter["#h"], [CHANNEL]);
  assert.equal(filter.since, now - CHANNEL_LIVE_BACKLOG_GRACE_SECONDS);
  assert.ok(filter.since < now);
});

// 39005 recounts ride this subscription and no other; a missing kind here means
// live thread badges never recount.
test("channel live filter carries thread-summary recounts", () => {
  const filter = buildChannelLiveFilter(CHANNEL, 1_725_100_000);
  assert.ok(filter.kinds.includes(39005));
});
