import assert from "node:assert/strict";
import test from "node:test";
import * as React from "react";
import { renderToStaticMarkup } from "react-dom/server";

import { PaneEmptyState } from "./PaneEmptyState.tsx";

test("narrow variant keeps a short title class and hides the paragraph", () => {
  const html = renderToStaticMarkup(
    React.createElement(PaneEmptyState, {
      description: "Reply in the thread to continue this branch.",
      narrowTitle: "No replies yet",
      testId: "thread-empty-state",
      title: "No replies in this branch yet",
    }),
  );
  assert.match(html, /data-testid="thread-empty-state"/);
  assert.match(html, /No replies yet/);
  assert.match(html, /No replies in this branch yet/);
  assert.match(html, /\[@container\(max-width:21\.25rem\)\]:hidden/);
  assert.match(html, /Reply in the thread/);
});
