import assert from "node:assert/strict";
import test from "node:test";
import * as React from "react";
import { renderToStaticMarkup } from "react-dom/server";

import { DeclaredPlansRail } from "./DeclaredPlansRail.tsx";
import { declaredPlanStatusLine } from "./DeclaredPlanCard.tsx";

const DEV = "aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111aaaa1111";
const SCOUT =
  "bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222bbbb2222";
const CLAUDE =
  "cccc3333cccc3333cccc3333cccc3333cccc3333cccc3333cccc3333cccc3333";
const NOW = Date.parse("2026-08-13T10:18:00.000Z");

test("rail renders one card per agent and never merges checklists", () => {
  const html = renderToStaticMarkup(
    React.createElement(DeclaredPlansRail, {
      plans: [
        {
          agentPubkey: DEV,
          agentName: "Hermes Dev",
          conversationId: "conv",
          entries: [
            { content: "Fetch Buzz 0.5.11 tags", status: "completed" },
            { content: "Compare ACP lifecycle", status: "in_progress" },
          ],
          updatedAt: "2026-08-13T10:16:00.000Z",
          source: "acp-plan",
          liveness: "working",
          unknown: false,
          sessionId: "sess-dev",
        },
        {
          agentPubkey: SCOUT,
          agentName: "Hermes Scout",
          conversationId: "conv",
          entries: [
            { content: "Inventory existing todo seams", status: "completed" },
          ],
          updatedAt: "2026-08-13T10:00:00.000Z",
          source: "acp-plan",
          liveness: "sleeping",
          unknown: false,
          sessionId: "sess-scout",
        },
        {
          agentPubkey: CLAUDE,
          agentName: "Claude",
          conversationId: "conv",
          entries: [],
          updatedAt: null,
          source: null,
          liveness: "disconnected",
          unknown: true,
          sessionId: null,
        },
      ],
    }),
  );

  assert.match(html, /data-testid="declared-plans-rail"/);
  assert.match(html, /data-layout="side"/);
  assert.match(html, new RegExp(`data-testid="declared-plan-card-${DEV}"`));
  assert.match(html, new RegExp(`data-testid="declared-plan-card-${SCOUT}"`));
  assert.match(html, new RegExp(`data-testid="declared-plan-card-${CLAUDE}"`));
  assert.match(html, /Hermes Dev/);
  assert.match(html, /Hermes Scout/);
  assert.match(html, /Compare ACP lifecycle/);
  assert.match(html, /in_progress/);
  assert.match(html, /No ACP plan or structured todo snapshot/);
  assert.match(html, /border-dashed/);
  assert.doesNotMatch(html, /contenteditable/);
});

test("status line names working, sleeping, and disconnected honestly", () => {
  assert.match(
    declaredPlanStatusLine(
      {
        agentPubkey: DEV,
        agentName: "Hermes Dev",
        conversationId: "conv",
        entries: [],
        updatedAt: "2026-08-13T10:16:00.000Z",
        source: "acp-plan",
        liveness: "working",
        unknown: false,
        sessionId: "sess",
      },
      NOW,
    ),
    /working · updated 2m ago/,
  );
  assert.match(
    declaredPlanStatusLine(
      {
        agentPubkey: SCOUT,
        agentName: "Hermes Scout",
        conversationId: "conv",
        entries: [],
        updatedAt: "2026-08-13T10:00:00.000Z",
        source: "acp-plan",
        liveness: "sleeping",
        unknown: false,
        sessionId: "sess",
      },
      NOW,
    ),
    /sleeping · last declared 18m ago/,
  );
  assert.equal(
    declaredPlanStatusLine(
      {
        agentPubkey: CLAUDE,
        agentName: "Claude",
        conversationId: "conv",
        entries: [],
        updatedAt: null,
        source: null,
        liveness: "disconnected",
        unknown: true,
        sessionId: null,
      },
      NOW,
    ),
    "disconnected · plan unknown",
  );
});

test("stacked layout is full width under the header, never a squeezed side column", () => {
  const html = renderToStaticMarkup(
    React.createElement(DeclaredPlansRail, {
      layout: "stacked",
      plans: [
        {
          agentPubkey: DEV,
          agentName: "Hermes Dev",
          conversationId: "conv",
          entries: [{ content: "Stay readable", status: "in_progress" }],
          updatedAt: "2026-08-13T10:16:00.000Z",
          source: "acp-plan",
          liveness: "working",
          unknown: false,
          sessionId: "sess-dev",
        },
      ],
    }),
  );
  assert.match(html, /data-layout="stacked"/);
  assert.match(html, /max-h-48/);
  assert.doesNotMatch(html, /border-l /);
});
