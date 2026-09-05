import { after, before } from "node:test";

import { JSDOM } from "jsdom";

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});

/** event name → registered Tauri callback ids */
const eventHandlers = new Map();
/** callback id → handler function */
const callbacks = new Map();
let nextCallbackId = 1;
export const nativeCalls = [];
export const invokeOverrides = new Map();

export const SPEAKER_PUBKEY = "0f".repeat(32);

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
      nativeCalls.push({ command, payload });
      if (invokeOverrides.has(command))
        return invokeOverrides.get(command)(payload);
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

export function emitTauriEvent(event, payload) {
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

export async function mountProviderWithProbes() {
  // Unlisten is a no-op in the stub; drop handlers so a previous mount's
  // listeners cannot receive this mount's ticks.
  eventHandlers.clear();
  callbacks.clear();
  nativeCalls.length = 0;
  invokeOverrides.clear();
  const React = await import("react");
  const { act, cleanup, render } = await import("@testing-library/react");
  const { HuddleProvider, useHuddle, useHuddleLevels } = await import(
    "./HuddleContext.tsx"
  );

  const renders = { huddle: 0, levels: 0 };
  const state = { current: null };

  function HuddleConsumer() {
    const huddle = useHuddle();
    state.current = huddle;
    const { isStarting } = huddle;
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

  return { act, cleanup, renders, state };
}
