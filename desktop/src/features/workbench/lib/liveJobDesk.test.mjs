import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { workbenchHref } from "./workbenchRoutes.ts";
import {
  collectLiveJobSignals,
  hrefForWorkbenchPlace,
  isLiveAgentJob,
  resolveWorkbenchPlace,
  selectedSessionFromLocation,
  shouldShowLiveJobDesk,
} from "./liveJobDesk.ts";

const CHANNEL = "1c7e1c02-87bb-5e88-b2da-5a7a9432d0c9";
const ROOT = "1".repeat(64);

describe("live job desk (#219)", () => {
  it("no job ⇒ no desk", () => {
    assert.equal(shouldShowLiveJobDesk([]), false);
    assert.equal(
      shouldShowLiveJobDesk([{ kind: "mission", status: "idle" }]),
      false,
    );
    assert.equal(
      shouldShowLiveJobDesk([{ kind: "mission", status: "sleeping" }]),
      false,
    );
    assert.equal(
      shouldShowLiveJobDesk([{ kind: "mission", status: "ready" }]),
      false,
    );
    assert.equal(
      shouldShowLiveJobDesk([{ kind: "mission", status: "failed" }]),
      false,
    );
    assert.equal(isLiveAgentJob("idle"), false);
    assert.equal(isLiveAgentJob("sleeping"), false);
    assert.equal(isLiveAgentJob("ready"), false);
    assert.equal(isLiveAgentJob("failed"), false);
  });

  it("a live job (working or stuck) shows the desk", () => {
    assert.equal(isLiveAgentJob("working"), true);
    assert.equal(isLiveAgentJob("needs-you"), true);
    assert.equal(
      shouldShowLiveJobDesk([{ kind: "mission", status: "working" }]),
      true,
    );
    assert.equal(
      shouldShowLiveJobDesk([{ kind: "mission", status: "needs-you" }]),
      true,
    );
    assert.equal(shouldShowLiveJobDesk([{ kind: "active-turn" }]), true);
    assert.equal(shouldShowLiveJobDesk([{ kind: "pending-user-input" }]), true);
  });

  it("workbench is not a picker destination", () => {
    assert.deepEqual(resolveWorkbenchPlace(), { kind: "none" });
    assert.equal(hrefForWorkbenchPlace({ kind: "none" }), "/");
    assert.equal(workbenchHref(), "/");
    assert.doesNotMatch(workbenchHref(), /\/workbench\/?$/);
    assert.doesNotMatch(hrefForWorkbenchPlace({ kind: "none" }), /workbench/);
  });

  it("a thread workbench href is the channel session, not a third place", () => {
    assert.deepEqual(resolveWorkbenchPlace(CHANNEL, ROOT), {
      kind: "channel-session",
      channelId: CHANNEL,
      threadRootId: ROOT,
    });
    assert.equal(
      hrefForWorkbenchPlace({
        kind: "channel-session",
        channelId: CHANNEL,
        threadRootId: ROOT,
      }),
      `/channels/${CHANNEL}?thread=${ROOT}`,
    );
    assert.equal(
      workbenchHref(CHANNEL, ROOT),
      `/channels/${CHANNEL}?thread=${ROOT}`,
    );
    assert.doesNotMatch(workbenchHref(CHANNEL, ROOT), /\/workbench\//);
  });

  it("collects live-job signals without inventing a job", () => {
    assert.deepEqual(
      collectLiveJobSignals({
        hasActiveTurn: false,
        hasPendingUserInput: false,
      }),
      [],
    );
    assert.deepEqual(
      collectLiveJobSignals({
        hasActiveTurn: true,
        hasPendingUserInput: false,
        missionStatus: "idle",
      }),
      [{ kind: "active-turn" }, { kind: "mission", status: "idle" }],
    );
  });

  it("reads the channel session from the location, not a workbench place", () => {
    assert.deepEqual(
      selectedSessionFromLocation({
        pathname: `/channels/${CHANNEL}`,
        search: { thread: ROOT },
      }),
      { channelId: CHANNEL, threadRootId: ROOT },
    );
    assert.deepEqual(
      selectedSessionFromLocation({
        pathname: "/workbench",
        search: {},
      }),
      { channelId: null, threadRootId: null },
    );
  });
});
