import assert from "node:assert/strict";
import test from "node:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { TooltipProvider } from "@/shared/ui/tooltip";
import { renderMessageStatusMetadata } from "./message-status-metadata.tsx";

function markup(input) {
  return renderToStaticMarkup(
    React.createElement(
      TooltipProvider,
      null,
      renderMessageStatusMetadata({ editAsUndoState: null, ...input }),
    ),
  );
}

test("ordinary accepted messages have no status metadata", () => {
  assert.equal(renderMessageStatusMetadata({ editAsUndoState: null }), null);
});

test("withdrawn and already-read edits retain their distinct outcomes", () => {
  assert.match(
    markup({ editAsUndoState: "withdrawn" }),
    /Request withdrawn — agent never ran/,
  );
  assert.match(
    markup({ editAsUndoState: "too-late" }),
    /Agent already read the original/,
  );
});

test("pending and edited indicators remain alongside the edit outcome", () => {
  const html = markup({
    pending: true,
    edited: true,
    editAsUndoState: "withdrawn",
  });
  assert.match(html, /Sending…/);
  assert.match(html, /\(edited\)/);
  assert.match(html, /message-edit-as-undo-status/);
});
