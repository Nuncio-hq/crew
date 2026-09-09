import assert from "node:assert/strict";
import { after, afterEach, before, test } from "node:test";
import { JSDOM } from "jsdom";

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});
let React, render, fireEvent, screen, cleanup, Rows;
before(async () => {
  Object.assign(globalThis, {
    window: dom.window,
    document: dom.window.document,
    IS_REACT_ACT_ENVIRONMENT: true,
  });
  for (const key of [
    "HTMLElement",
    "HTMLInputElement",
    "Node",
    "NodeFilter",
    "MutationObserver",
    "CustomEvent",
    "Event",
    "DocumentFragment",
  ])
    globalThis[key] = dom.window[key];
  globalThis.getComputedStyle = dom.window.getComputedStyle.bind(dom.window);
  Object.defineProperty(globalThis, "navigator", {
    configurable: true,
    value: dom.window.navigator,
  });
  React = await import("react");
  ({ render, fireEvent, screen, cleanup } = await import(
    "@testing-library/react"
  ));
  ({ UserProfileAgentManagementRows: Rows } = await import(
    "./UserProfileAgentManagementRows.tsx"
  ));
});
afterEach(() => cleanup());
after(() => dom.window.close());
const agent = (pubkey) => ({
  pubkey,
  name: "Same name",
  backend: { type: "local" },
  status: "stopped",
});
function tree(managedAgent, onDeleteAgent) {
  return React.createElement(Rows, {
    canArchiveAgent: false,
    canDeleteAgent: true,
    isDeletePending: false,
    managedAgent,
    onDeleteAgent,
  });
}

test("an open delete confirmation cannot transfer to a same-name replacement instance", () => {
  const deleted = [];
  const view = render(tree(agent("a".repeat(64)), () => deleted.push("a")));
  fireEvent.click(screen.getByTestId("user-profile-delete-agent-row"));
  assert.ok(screen.getByRole("alertdialog"));
  view.rerender(tree(agent("b".repeat(64)), () => deleted.push("b")));
  assert.equal(
    Boolean(screen.queryByRole("alertdialog")),
    false,
    "replacement requires its own confirmation",
  );
  assert.deepEqual(deleted, []);
  fireEvent.click(screen.getByTestId("user-profile-delete-agent-row"));
  fireEvent.click(screen.getByTestId("agent-delete-confirm-action"));
  assert.deepEqual(deleted, ["b"]);
});

test("delete confirmation names the instance and Cancel performs no removal", () => {
  const deleted = [];
  render(tree(agent("a".repeat(64)), () => deleted.push("a")));
  fireEvent.click(screen.getByTestId("user-profile-delete-agent-row"));
  assert.match(screen.getByRole("alertdialog").textContent, /Same name/);
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  assert.deepEqual(deleted, []);
});

test("losing local delete authority dismisses the pending confirmation", () => {
  const deleted = [];
  const props = {
    canArchiveAgent: false,
    canDeleteAgent: true,
    isDeletePending: false,
    managedAgent: agent("a".repeat(64)),
    onDeleteAgent: () => deleted.push("a"),
  };
  const view = render(React.createElement(Rows, props));
  fireEvent.click(screen.getByTestId("user-profile-delete-agent-row"));
  view.rerender(React.createElement(Rows, { ...props, canDeleteAgent: false }));
  assert.equal(Boolean(screen.queryByRole("alertdialog")), false);
  assert.deepEqual(deleted, []);
});

test("renaming the same instance retains its exact deletion target", () => {
  const deleted = [];
  const selected = agent("a".repeat(64));
  const remove = () => deleted.push(selected.pubkey);
  const view = render(tree(selected, remove));
  fireEvent.click(screen.getByTestId("user-profile-delete-agent-row"));
  view.rerender(tree({ ...selected, name: "Renamed" }, remove));
  assert.match(screen.getByRole("alertdialog").textContent, /Renamed/);
  fireEvent.click(screen.getByTestId("agent-delete-confirm-action"));
  assert.deepEqual(deleted, [selected.pubkey]);
});
