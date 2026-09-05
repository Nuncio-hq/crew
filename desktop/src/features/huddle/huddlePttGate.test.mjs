import assert from "node:assert/strict";
import { test } from "node:test";
import {
  invokeOverrides,
  mountProviderWithProbes,
  nativeCalls,
} from "./huddle-provider-fixture.mjs";

for (const command of ["start_huddle", "join_huddle"]) {
  test(`${command} initializes push-to-talk muted before native audio starts`, async () => {
    const { act, cleanup, state } = await mountProviderWithProbes();
    let rejectStart;
    let operation;
    const pending = new Promise((_, reject) => {
      rejectStart = reject;
    });
    invokeOverrides.set(command, () => pending);
    try {
      await act(async () => {
        await state.current.setVoiceInputMode("push_to_talk");
      });
      await act(async () => {
        operation =
          command === "start_huddle"
            ? state.current.startHuddle("parent-channel", [])
            : state.current.joinHuddle("parent-channel", "ephemeral-channel");
        await Promise.resolve();
      });
      assert.equal(state.current.isStarting, true);
      assert.equal(state.current.isMuted, true);
      assert.equal(
        nativeCalls.some((call) => call.command === command),
        true,
      );
    } finally {
      await act(async () => {
        rejectStart(new Error("Stop fixture before opening audio"));
        await assert.rejects(operation, /Stop fixture before opening audio/);
      });
      cleanup();
    }
  });
}

test("changing voice mode resynchronizes the native manual transcription gate", async () => {
  const { act, cleanup, state } = await mountProviderWithProbes();
  try {
    nativeCalls.length = 0;
    await act(async () => {
      await state.current.setVoiceInputMode("push_to_talk");
    });
    assert.deepEqual(
      nativeCalls.filter(
        (call) => call.command === "set_huddle_manual_mic_unmuted",
      ),
      [
        {
          command: "set_huddle_manual_mic_unmuted",
          payload: { enabled: true },
        },
      ],
    );
  } finally {
    cleanup();
  }
});
