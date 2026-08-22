import assert from "node:assert/strict";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { runChannelFirstIaCheck } from "../../../scripts/check-channel-first-ia-core.mjs";
import { CHANNEL_FIRST_IA_RULES } from "../../scripts/check-channel-first-ia.mjs";

const repoRoot = path.resolve(import.meta.dirname, "../../..");

function rule(file, overrides = {}) {
  return {
    path: file,
    banned: [],
    required: [],
    ...overrides,
  };
}

async function withFixture(files, callback) {
  const root = await mkdtemp(path.join(os.tmpdir(), "channel-first-ia-"));
  try {
    for (const [file, source] of Object.entries(files)) {
      const filePath = path.join(root, file);
      await mkdir(path.dirname(filePath), { recursive: true });
      await writeFile(filePath, source);
    }
    return await callback(root);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

test("seeded AppSidebar Projects selector is caught", async () => {
  const violations = await withFixture(
    {
      "src/features/sidebar/ui/AppSidebar.tsx":
        "export const AppSidebar = () => onSelectProjects();\n",
    },
    (projectRoot) =>
      runChannelFirstIaCheck({
        projectRoot,
        rules: [
          rule("src/features/sidebar/ui/AppSidebar.tsx", {
            banned: [
              {
                marker: "onSelectProjects",
                reason: "preserve channel-first navigation (#223, D-066)",
              },
            ],
          }),
        ],
        throwOnViolations: false,
      }),
  );

  assert.equal(violations.length, 1);
  assert.equal(violations[0].kind, "banned");
  assert.equal(violations[0].marker, "onSelectProjects");
  assert.match(violations[0].reason, /#223/);
});

test("seeded message header without LiveJobDesk is caught", async () => {
  const violations = await withFixture(
    {
      "src/features/messages/ui/message-thread-panel-head.tsx":
        "export const Header = () => null;\n",
    },
    (projectRoot) =>
      runChannelFirstIaCheck({
        projectRoot,
        rules: [
          rule("src/features/messages/ui/message-thread-panel-head.tsx", {
            required: [
              {
                marker: "LiveJobDesk",
                reason: "preserve the live-job desk (#219, D-065)",
              },
            ],
          }),
        ],
        throwOnViolations: false,
      }),
  );

  assert.equal(violations.length, 1);
  assert.equal(violations[0].kind, "required");
  assert.equal(violations[0].marker, "LiveJobDesk");
  assert.match(violations[0].reason, /#219/);
});

test("a missing guarded file is caught", async () => {
  const violations = await withFixture({}, (projectRoot) =>
    runChannelFirstIaCheck({
      projectRoot,
      rules: [
        rule("src/app/routes/workbench.tsx", {
          required: [
            {
              marker: "redirect",
              reason: "workbench must remain redirect-only (#219)",
            },
          ],
        }),
      ],
      throwOnViolations: false,
    }),
  );

  assert.equal(violations.length, 1);
  assert.equal(violations[0].kind, "missing");
  assert.equal(violations[0].marker, "file");
});

test("a clean fixture has no channel-first IA violations", async () => {
  const violations = await withFixture(
    {
      "src/app/AppShell.helpers.ts": 'return { selectedView: "inbox" };\n',
    },
    (projectRoot) =>
      runChannelFirstIaCheck({
        projectRoot,
        rules: [
          rule("src/app/AppShell.helpers.ts", {
            banned: [
              {
                marker: /selectedView\s*:\s*"workbench"/,
                reason: "keep workbench mapped to a channel session (#219)",
              },
            ],
            required: [
              {
                marker: "selectedView",
                reason: "preserve the shell view selection contract",
              },
            ],
          }),
        ],
        throwOnViolations: false,
      }),
  );

  assert.deepEqual(violations, []);
});

test("current Crew tree satisfies the configured channel-first IA rules", async () => {
  const violations = await runChannelFirstIaCheck({
    projectRoot: path.join(repoRoot, "desktop"),
    rules: CHANNEL_FIRST_IA_RULES,
    throwOnViolations: false,
  });

  assert.deepEqual(violations, []);
});

test("desktop check chain includes the channel-first IA leg", async () => {
  const packageJson = await import(
    `${path.join(repoRoot, "desktop/package.json")}?contract=${Date.now()}`,
    { with: { type: "json" } }
  );

  assert.match(
    packageJson.default.scripts.check,
    /pnpm check:channel-first-ia/,
  );
});
