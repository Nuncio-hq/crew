import assert from "node:assert/strict";
import { after, afterEach, test } from "node:test";
import { JSDOM } from "jsdom";

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});
Object.assign(globalThis, {
  window: dom.window,
  document: dom.window.document,
  IS_REACT_ACT_ENVIRONMENT: true,
});
for (const name of Object.getOwnPropertyNames(dom.window)) {
  if (
    /^(HTML|SVG)/.test(name) ||
    /Event$/.test(name) ||
    ["Node", "NodeFilter", "MutationObserver", "getComputedStyle"].includes(
      name,
    )
  ) {
    Object.defineProperty(globalThis, name, {
      configurable: true,
      writable: true,
      value: dom.window[name],
    });
  }
}
Object.defineProperty(globalThis, "navigator", {
  configurable: true,
  value: dom.window.navigator,
});
const ipcCalls = [];
window.__TAURI_INTERNALS__ = {
  invoke: async (command, args) => {
    ipcCalls.push({ command, args });
    assert.equal(command, "estimate_hermes_profile_archive");
    return { included_bytes: 1024, excluded_bytes: 0, entry_count: 2 };
  },
};

const React = await import("react");
const { act, cleanup, fireEvent, render, screen } = await import(
  "@testing-library/react"
);
const { PersonaDeleteDialog } = await import("./PersonaDeleteDialog.tsx");
const persona = {
  id: "scout-persona",
  displayName: "Scout",
  respondTo: "owner",
};

afterEach(() => {
  cleanup();
  ipcCalls.length = 0;
});
after(() => dom.window.close());

function mountDialog() {
  const confirmations = [];
  const changes = [];
  function Harness() {
    const [open, setOpen] = React.useState(true);
    return React.createElement(
      React.Fragment,
      null,
      React.createElement("button", { onClick: () => setOpen(true) }, "Reopen"),
      React.createElement(PersonaDeleteDialog, {
        open,
        persona,
        instanceCount: 2,
        hermesProfiles: ["scout", "scout"],
        onConfirm: (...args) => confirmations.push(args),
        onOpenChange: (next) => {
          changes.push(next);
          setOpen(next);
        },
      }),
    );
  }
  render(React.createElement(Harness));
  return { confirmations, changes };
}

test("Cancel discards archive consent and reopening defaults to Keep", async () => {
  const state = mountDialog();
  await act(async () =>
    fireEvent.click(screen.getByTestId("hermes-profile-offboard-archive")),
  );
  await act(async () =>
    fireEvent.change(
      screen.getByRole("textbox", { name: "Optional archive reason" }),
      {
        target: { value: "Retire this role" },
      },
    ),
  );
  await act(async () =>
    fireEvent.click(screen.getByRole("button", { name: "Cancel" })),
  );
  assert.deepEqual(state.confirmations, []);
  assert.equal(state.changes.at(-1), false);
  await act(async () =>
    fireEvent.click(screen.getByRole("button", { name: "Reopen" })),
  );
  assert.equal(
    screen.getByTestId("hermes-profile-offboard-keep").checked,
    true,
  );
  await act(async () =>
    fireEvent.click(
      screen.getByRole("button", { name: "Delete", exact: true }),
    ),
  );
  assert.deepEqual(state.confirmations, [
    [
      persona,
      {
        archiveHermesProfiles: false,
        hermesProfileReason: "",
      },
    ],
  ]);
});

test("Delete keeps Hermes profile data unless Archive is explicitly selected", async () => {
  const state = mountDialog();
  assert.equal(
    screen.getByTestId("hermes-profile-offboard-keep").checked,
    true,
  );
  await act(async () =>
    fireEvent.click(
      screen.getByRole("button", { name: "Delete", exact: true }),
    ),
  );
  assert.deepEqual(state.confirmations, [
    [
      persona,
      {
        archiveHermesProfiles: false,
        hermesProfileReason: "",
      },
    ],
  ]);
});

test("Archive confirmation forwards the selected choice and exact reason once", async () => {
  const state = mountDialog();
  await act(async () =>
    fireEvent.click(screen.getByTestId("hermes-profile-offboard-archive")),
  );
  await act(async () =>
    fireEvent.change(
      screen.getByRole("textbox", { name: "Optional archive reason" }),
      {
        target: { value: "Retire Scout — keep its memories" },
      },
    ),
  );
  await act(async () =>
    fireEvent.click(
      screen.getByRole("button", { name: "Delete", exact: true }),
    ),
  );
  assert.deepEqual(state.confirmations, [
    [
      persona,
      {
        archiveHermesProfiles: true,
        hermesProfileReason: "Retire Scout — keep its memories",
      },
    ],
  ]);
  assert.ok(
    ipcCalls.some(
      ({ command, args }) =>
        command === "estimate_hermes_profile_archive" &&
        args.profile === "scout",
    ),
  );
});
