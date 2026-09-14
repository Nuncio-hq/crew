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
let deletePersonaFailure = null;
window.__TAURI_INTERNALS__ = {
  invoke: async (command, args) => {
    ipcCalls.push({ command, args });
    if (command === "estimate_hermes_profile_archive") {
      return { included_bytes: 1024, excluded_bytes: 0, entry_count: 2 };
    }
    if (command === "delete_persona") {
      if (deletePersonaFailure) throw deletePersonaFailure;
      return undefined;
    }
    if (command === "archive_hermes_profile") {
      return {
        status: "archived",
        id: "scout-archive",
        profile: args.profile,
        included_bytes: 1024,
        archive_bytes: 1024,
        skipped_link_count: 0,
      };
    }
    throw new Error(`Unexpected Tauri command: ${command}`);
  },
};

const React = await import("react");
const { act, cleanup, fireEvent, render, screen, waitFor } = await import(
  "@testing-library/react"
);
const { PersonaDeleteDialog } = await import("./PersonaDeleteDialog.tsx");
const { useProfileHermesAwareDeletes } = await import(
  "../../profile/ui/useProfileHermesAwareDeletes.ts"
);
const { deletePersona } = await import("../../../shared/api/tauriPersonas.ts");
const persona = {
  id: "scout-persona",
  displayName: "Scout",
  respondTo: "owner",
};

afterEach(() => {
  cleanup();
  ipcCalls.length = 0;
  deletePersonaFailure = null;
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

function mountProductionDeleteHandler() {
  const state = { closed: 0, pending: Promise.resolve() };
  function Harness() {
    const [open, setOpen] = React.useState(true);
    const { handleConfirmDeletePersona } = useProfileHermesAwareDeletes({
      managedAgent: undefined,
      managedAgents: [{ personaId: persona.id, hermesProfile: "scout" }],
      deleteManagedAgentRecord: async () => ({ cancelled: false }),
      deletePersona,
      onClose: () => {
        state.closed += 1;
      },
      setPersonaToDelete: (next) => setOpen(next !== null),
    });
    return React.createElement(
      React.Fragment,
      null,
      React.createElement("button", { onClick: () => setOpen(true) }, "Reopen"),
      React.createElement(PersonaDeleteDialog, {
        open,
        persona,
        instanceCount: 1,
        hermesProfiles: ["scout"],
        onConfirm: (selectedPersona, options) => {
          state.pending = handleConfirmDeletePersona(selectedPersona, options);
        },
        onOpenChange: setOpen,
      }),
    );
  }
  render(React.createElement(Harness));
  return state;
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

test("production persona delete handler applies Keep and Archive at the IPC seam", async () => {
  const state = mountProductionDeleteHandler();
  await act(async () =>
    fireEvent.click(
      screen.getByRole("button", { name: "Delete", exact: true }),
    ),
  );
  await act(async () => state.pending);
  await waitFor(() =>
    assert.equal(
      ipcCalls.filter(({ command }) => command === "delete_persona").length,
      1,
    ),
  );
  await waitFor(() => assert.equal(state.closed, 1));
  assert.deepEqual(
    ipcCalls.find(({ command }) => command === "delete_persona"),
    {
      command: "delete_persona",
      args: { id: persona.id },
    },
  );
  assert.equal(
    ipcCalls.filter(({ command }) => command === "archive_hermes_profile")
      .length,
    0,
  );

  await waitFor(() => assert.equal(screen.queryByRole("dialog"), null));
  await act(async () =>
    fireEvent.click(screen.getByRole("button", { name: "Reopen" })),
  );
  await act(async () =>
    fireEvent.click(screen.getByTestId("hermes-profile-offboard-archive")),
  );
  await act(async () =>
    fireEvent.change(
      screen.getByRole("textbox", { name: "Optional archive reason" }),
      { target: { value: "Retire Scout" } },
    ),
  );
  await act(async () =>
    fireEvent.click(
      screen.getByRole("button", { name: "Delete", exact: true }),
    ),
  );
  await act(async () => state.pending);
  await waitFor(() =>
    assert.equal(
      ipcCalls.filter(({ command }) => command === "archive_hermes_profile")
        .length,
      1,
    ),
  );
  assert.deepEqual(
    ipcCalls
      .filter(
        ({ command }) =>
          command === "delete_persona" || command === "archive_hermes_profile",
      )
      .map(({ command }) => command),
    ["delete_persona", "delete_persona", "archive_hermes_profile"],
  );
  assert.deepEqual(
    ipcCalls.find(({ command }) => command === "archive_hermes_profile"),
    {
      command: "archive_hermes_profile",
      args: { profile: "scout", reason: "Retire Scout" },
    },
  );
  await waitFor(() => assert.equal(state.closed, 2));

  await waitFor(() => assert.equal(screen.queryByRole("dialog"), null));
  deletePersonaFailure = new Error("persona delete failed");
  await act(async () =>
    fireEvent.click(screen.getByRole("button", { name: "Reopen" })),
  );
  await act(async () =>
    fireEvent.click(screen.getByTestId("hermes-profile-offboard-archive")),
  );
  await act(async () =>
    fireEvent.change(
      screen.getByRole("textbox", { name: "Optional archive reason" }),
      { target: { value: "Should not archive" } },
    ),
  );
  await act(async () =>
    fireEvent.click(
      screen.getByRole("button", { name: "Delete", exact: true }),
    ),
  );
  await act(async () => state.pending);
  await waitFor(() =>
    assert.equal(
      ipcCalls.filter(({ command }) => command === "delete_persona").length,
      3,
    ),
  );
  assert.equal(
    ipcCalls.filter(({ command }) => command === "archive_hermes_profile")
      .length,
    1,
    "a failed persona delete must not archive its Hermes profile",
  );
  assert.equal(state.closed, 2);
});
