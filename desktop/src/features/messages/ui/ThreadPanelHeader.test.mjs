import assert from "node:assert/strict";
import test from "node:test";
import * as React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { MessageThreadPanelHeader } from "./MessageThreadPanelSkeleton.tsx";
import { ThreadPanelOrientationTitle } from "./ThreadPanelOrientation.tsx";

function header(props = {}) {
  return renderToStaticMarkup(
    React.createElement(MessageThreadPanelHeader, {
      isFocusMode: false,
      isSinglePanelView: false,
      onClose() {},
      ...props,
    }),
  );
}

test("orientation fallback stays within one header heading", () => {
  const previousWindow = globalThis.window;
  globalThis.window = {};
  try {
    const html = header({
      titleContent: React.createElement(ThreadPanelOrientationTitle, {
        breadcrumb: null,
      }),
    });
    assert.equal((html.match(/<h2\b/g) ?? []).length, 1);
    assert.match(html, /<h2[^>]*>Thread<\/h2>/);
  } finally {
    globalThis.window = previousWindow;
  }
});

test("explicit channel header keeps its navigation action", () => {
  const html = header({
    headerTitle: "Engineering",
    headerTitleAriaLabel: "Open Engineering channel",
    onHeaderTitleClick() {},
  });
  assert.match(html, /aria-label="Open Engineering channel"/);
  assert.match(html, /data-testid="message-thread-open-channel"/);
  assert.match(html, />Engineering<\/button>/);
  assert.equal((html.match(/<h2\b/g) ?? []).length, 1);
});

test("single-column headers show Back while focus drawers use their scrim", () => {
  assert.match(header({ isSinglePanelView: true }), /message-thread-back/);
  assert.doesNotMatch(
    header({ isSinglePanelView: true, isFocusMode: true }),
    /message-thread-back/,
  );
  assert.match(
    header({ isFocusMode: true, showBackButton: true }),
    /message-thread-back/,
  );
});
