import assert from "node:assert/strict";
import { after, afterEach, before, test } from "node:test";
import { JSDOM } from "jsdom";

const dom = new JSDOM("<!doctype html><html><body></body></html>", {
  url: "http://localhost",
});
before(() => {
  Object.assign(globalThis, {
    document: dom.window.document,
    HTMLElement: dom.window.HTMLElement,
    IS_REACT_ACT_ENVIRONMENT: true,
    window: dom.window,
  });
});
afterEach(async () => {
  const { cleanup } = await import("@testing-library/react");
  cleanup();
  document.body.replaceChildren();
});
after(() => dom.window.close());

async function mountAutofocus() {
  const React = await import("react");
  const { render } = await import("@testing-library/react");
  const { useComposerAutofocus } = await import("./useComposerAutofocus.ts");
  let focused = 0;
  const focus = () => {
    focused++;
  };
  function Harness({ draftKey, ready }) {
    useComposerAutofocus(ready ? focus : () => {}, draftKey, false);
    return null;
  }
  const view = render(
    React.createElement(Harness, { draftKey: "first", ready: false }),
  );
  return {
    ready: (draftKey = "first") =>
      view.rerender(React.createElement(Harness, { draftKey, ready: true })),
    focused: () => focused,
  };
}

for (const role of ["dialog", "menu", "listbox"]) {
  test(`late editor hydration preserves focused ${role} control`, async () => {
    const harness = await mountAutofocus();
    const overlay = document.createElement("div");
    overlay.setAttribute("role", role);
    const control = document.createElement("button");
    control.setAttribute("role", role === "dialog" ? "switch" : "menuitem");
    overlay.append(control);
    document.body.append(overlay);
    control.focus();
    harness.ready();
    assert.equal(harness.focused(), 0);
    assert.equal(document.activeElement, control);
  });
}

test("channel navigation still autofocuses from a sidebar button", async () => {
  const harness = await mountAutofocus();
  const button = document.createElement("button");
  document.body.append(button);
  button.focus();
  harness.ready("next-channel");
  assert.equal(harness.focused(), 1);
});
