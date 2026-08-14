import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { runPaneResponsiveCheck } from "../../../../scripts/check-pane-responsive-core.mjs";

test("seeded P1 flex-1 without min-w-0 is caught", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "pane-responsive-"));
  const paneDir = path.join(root, "src/features/messages/ui");
  await mkdir(paneDir, { recursive: true });
  await writeFile(
    path.join(paneDir, "SeededSqueeze.tsx"),
    `export function SeededSqueeze() {
  return <div className="flex-1 text-sm">Buzztags</div>;
}
`,
  );
  try {
    const violations = await runPaneResponsiveCheck({
      projectRoot: root,
      throwOnViolations: false,
    });
    assert.equal(violations.length, 1);
    assert.equal(violations[0].pattern, "P1");
    assert.match(violations[0].rel, /SeededSqueeze/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("seeded P3 viewport breakpoint in a pane file is caught", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "pane-responsive-"));
  const paneDir = path.join(root, "src/features/messages/ui");
  await mkdir(paneDir, { recursive: true });
  await writeFile(
    path.join(paneDir, "SeededViewport.tsx"),
    `export function SeededViewport() {
  return <div className="hidden sm:flex">wide only</div>;
}
`,
  );
  try {
    const violations = await runPaneResponsiveCheck({
      projectRoot: root,
      throwOnViolations: false,
    });
    assert.equal(violations.length, 1);
    assert.equal(violations[0].pattern, "P3");
    assert.match(violations[0].detail, /sm:/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("flex-col flex-1 layout shell is not a P1 hit", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "pane-responsive-"));
  const paneDir = path.join(root, "src/features/messages/ui");
  await mkdir(paneDir, { recursive: true });
  await writeFile(
    path.join(paneDir, "ColumnShell.tsx"),
    `export function ColumnShell() {
  return <div className="flex min-h-0 flex-1 flex-col">ok</div>;
}
`,
  );
  try {
    const violations = await runPaneResponsiveCheck({
      projectRoot: root,
      throwOnViolations: false,
    });
    assert.equal(violations.length, 0);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("overflow-y-auto flex-1 is not a P1 hit", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "pane-responsive-"));
  const paneDir = path.join(root, "src/features/messages/ui");
  await mkdir(paneDir, { recursive: true });
  await writeFile(
    path.join(paneDir, "ScrollShell.tsx"),
    `export function ScrollShell() {
  return <div className="min-h-0 flex-1 overflow-y-auto">ok</div>;
}
`,
  );
  try {
    const violations = await runPaneResponsiveCheck({
      projectRoot: root,
      throwOnViolations: false,
    });
    assert.equal(violations.length, 0);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
