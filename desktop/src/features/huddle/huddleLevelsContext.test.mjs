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
import { after, before, test } from "node:test";

import { JSDOM } from "jsdom";

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});

/** event name → registered Tauri callback ids */
const eventHandlers = new Map();
/** callback id → handler function */
const callbacks = new Map();
let nextCallbackId = 1;

const SPEAKER_PUBKEY = "0f".repeat(32);

function installTauriInternals() {
  dom.window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener(event, eventId) {
      const listeners = eventHandlers.get(event) ?? [];
      eventHandlers.set(
        event,
        listeners.filter((id) => id !== eventId),
      );
    },
  };
  dom.window.__TAURI_INTERNALS__ = {
    transformCallback(callback) {
      const id = nextCallbackId++;
      callbacks.set(id, callback);
      return id;
    },
    async invoke(command, payload) {
      if (command === "plugin:event|listen") {
        const listeners = eventHandlers.get(payload.event) ?? [];
        listeners.push(payload.handler);
        eventHandlers.set(payload.event, listeners);
        return 1;
      }
      if (command === "plugin:event|unlisten") return undefined;
      if (command === "plugin:event|emit") return undefined;
      if (command === "get_voice_input_mode") return "voice_activity";
      return undefined;
    },
  };
}

function emitTauriEvent(event, payload) {
  for (const handlerId of eventHandlers.get(event) ?? []) {
    callbacks.get(handlerId)?.({ event, id: handlerId, payload });
  }
}

before(() => {
  installTauriInternals();
  Object.assign(globalThis, {
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    IS_REACT_ACT_ENVIRONMENT: true,
  });
  for (const key of ["window", "navigator"]) {
    Object.defineProperty(globalThis, key, {
      configurable: true,
      value: key === "window" ? dom.window : dom.window.navigator,
    });
  }
  // The provider only enumerates devices; no worklet or media stream is opened
  // because no huddle is ever started in these tests.
  Object.defineProperty(dom.window.navigator, "mediaDevices", {
    configurable: true,
    value: {
      addEventListener() {},
      removeEventListener() {},
      enumerateDevices: async () => [],
    },
  });
});

after(() => dom.window.close());

async function mountProviderWithProbes() {
  // Unlisten is a no-op in the stub; drop handlers so a previous mount's
  // listeners cannot receive this mount's ticks.
  eventHandlers.clear();
  callbacks.clear();
  const React = await import("react");
  const { act, cleanup, render } = await import("@testing-library/react");
  const { HuddleProvider, useHuddle, useHuddleLevels } = await import(
    "./HuddleContext.tsx"
  );

  const renders = { huddle: 0, levels: 0 };

  function HuddleConsumer() {
    const { isStarting } = useHuddle();
    renders.huddle += 1;
    return React.createElement("span", null, String(isStarting));
  }

  function LevelsConsumer() {
    const { speakerLevels } = useHuddleLevels();
    renders.levels += 1;
    return React.createElement(
      "span",
      null,
      JSON.stringify(Object.keys(speakerLevels).length),
    );
  }

  await act(async () => {
    render(
      React.createElement(
        HuddleProvider,
        null,
        React.createElement(HuddleConsumer),
        React.createElement(LevelsConsumer),
      ),
    );
  });

  return { act, cleanup, renders };
}

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
