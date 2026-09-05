import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import {
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  realpathSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { policy as desktopPolicy } from "../desktop/scripts/check-file-sizes.mjs";
import { policy as mobilePolicy } from "../mobile/scripts/check-file-sizes.mjs";
import { policy as webPolicy } from "../web/scripts/check-file-sizes.mjs";
import {
  allowedLineCount,
  countLines,
  countRecordedLines,
  evaluateBaselineFileSize,
  evaluateFileSize,
  parseChangedFiles,
  parseFileSizeBaselines,
  resolveBaseRef,
  runFileSizeCheck,
} from "./check-file-sizes-core.mjs";

const repoRoot = path.resolve(import.meta.dirname, "..");

function git(repo, ...args) {
  return execFileSync("git", args, { cwd: repo, encoding: "utf8" }).trim();
}

function createEntrypointFixture({
  surface,
  files,
  lineDelta = 1,
  symlinkEntrypoint = false,
}) {
  const repo = realpathSync(
    mkdtempSync(path.join(tmpdir(), `file-size-${surface}-`)),
  );
  const scriptsDir = path.join(repo, "scripts");
  const surfaceScriptsDir = path.join(repo, surface, "scripts");
  mkdirSync(scriptsDir, { recursive: true });
  mkdirSync(surfaceScriptsDir, { recursive: true });
  copyFileSync(
    path.join(repoRoot, "scripts/check-file-sizes-core.mjs"),
    path.join(scriptsDir, "check-file-sizes-core.mjs"),
  );
  for (const fileName of ["check-file-sizes.mjs", "file-size-policy.mjs"]) {
    copyFileSync(
      path.join(repoRoot, surface, "scripts", fileName),
      path.join(surfaceScriptsDir, fileName),
    );
  }

  git(repo, "init", "-b", "main");
  git(repo, "config", "user.name", "Test");
  git(repo, "config", "user.email", "test@example.com");
  git(repo, "add", ".");
  git(repo, "commit", "-m", "base");
  const base = git(repo, "rev-parse", "HEAD");
  git(repo, "switch", "-c", "feature");

  for (const { relativeFile, maxLines } of files) {
    const governedFile = path.join(repo, surface, relativeFile);
    const lineCount = maxLines + lineDelta;
    mkdirSync(path.dirname(governedFile), { recursive: true });
    writeFileSync(governedFile, `${"line\n".repeat(lineCount - 1)}line`);
  }

  const realEntrypointPath = path.join(
    surfaceScriptsDir,
    "check-file-sizes.mjs",
  );
  let entrypointPath = realEntrypointPath;
  if (symlinkEntrypoint) {
    entrypointPath = path.join(repo, `${surface}-file-size-check.mjs`);
    symlinkSync(realEntrypointPath, entrypointPath);
  }
  const result = spawnSync(realpathSync(process.execPath), [entrypointPath], {
    cwd: repo,
    encoding: "utf8",
    env: { ...process.env, CHECK_FILE_SIZES_BASE: base },
  });
  return { result, relativeFiles: files.map(({ relativeFile }) => relativeFile) };
}

test("local base resolution uses the branch merge-base and fails without origin/main", () => {
  const repo = mkdtempSync(path.join(tmpdir(), "file-size-base-"));
  git(repo, "init", "-b", "main");
  git(repo, "config", "user.name", "Test");
  git(repo, "config", "user.email", "test@example.com");
  git(repo, "commit", "--allow-empty", "-m", "base");
  git(repo, "remote", "add", "origin", repo);
  git(repo, "fetch", "origin", "main:refs/remotes/origin/main");
  const base = git(repo, "rev-parse", "HEAD");
  git(repo, "switch", "-c", "feature");
  git(repo, "commit", "--allow-empty", "-m", "first branch commit");
  git(repo, "commit", "--allow-empty", "-m", "second branch commit");

  assert.equal(resolveBaseRef(repo, {}), base);
  git(repo, "update-ref", "-d", "refs/remotes/origin/main");
  assert.throws(
    () => resolveBaseRef(repo, {}),
    /Fetch origin\/main or set CHECK_FILE_SIZES_BASE/,
  );
});

const entrypointCases = [
  {
    surface: "desktop",
    files: [
      { relativeFile: "src-tauri/src/oversized.rs", maxLines: 1000 },
      { relativeFile: "src-tauri/crates/oversized.rs", maxLines: 1000 },
      { relativeFile: "src/app/oversized.ts", maxLines: 1000 },
      { relativeFile: "src/features/oversized.tsx", maxLines: 1000 },
      { relativeFile: "src/shared/api/oversized.ts", maxLines: 1000 },
      { relativeFile: "src/shared/context/oversized.tsx", maxLines: 1000 },
      { relativeFile: "src/shared/lib/oversized.ts", maxLines: 1000 },
      { relativeFile: "src/shared/ui/oversized.tsx", maxLines: 1000 },
      { relativeFile: "src/shared/styles/oversized.css", maxLines: 1000 },
    ],
  },
  {
    surface: "mobile",
    files: [{ relativeFile: "lib/oversized.dart", maxLines: 1000 }],
  },
  {
    surface: "web",
    files: [
      { relativeFile: "src/app/oversized.ts", maxLines: 1000 },
      { relativeFile: "src/features/oversized.tsx", maxLines: 1000 },
      { relativeFile: "src/shared/api/oversized.ts", maxLines: 1000 },
    ],
  },
];

test("surface entrypoints execute every production rule", () => {
  for (const fixture of entrypointCases) {
    const { result, relativeFiles } = createEntrypointFixture(fixture);
    assert.equal(
      result.status,
      1,
      `${fixture.surface} should reject every ceiling + 1: ${result.stderr || result.stdout}`,
    );
    for (const relativeFile of relativeFiles) {
      assert.ok(
        result.stderr.includes(relativeFile),
        `${fixture.surface} should report ${relativeFile}: ${result.stderr}`,
      );
    }
  }
});

test("surface entrypoints execute through symlinked paths", () => {
  for (const fixture of entrypointCases) {
    const { result, relativeFiles } = createEntrypointFixture({
      ...fixture,
      symlinkEntrypoint: true,
    });
    assert.equal(
      result.status,
      1,
      `${fixture.surface} symlink should reject ceiling + 1: ${result.stderr || result.stdout}`,
    );
    for (const relativeFile of relativeFiles) {
      assert.ok(
        result.stderr.includes(relativeFile),
        `${fixture.surface} symlink should report ${relativeFile}: ${result.stderr}`,
      );
    }
  }
});

test("surface entrypoints allow every production rule at its ceiling", () => {
  for (const fixture of entrypointCases) {
    const { result } = createEntrypointFixture({ ...fixture, lineDelta: 0 });
    assert.equal(
      result.status,
      0,
      `${fixture.surface} should allow every ceiling: ${result.stderr || result.stdout}`,
    );
  }
});

test("counts empty, LF, and CRLF content with the existing semantics", () => {
  assert.equal(countLines(""), 0);
  assert.equal(countLines("one\n"), 2);
  assert.equal(countLines("one\r\ntwo"), 2);
});

test("recorded line counts use wc -l semantics alongside ordinary counts", () => {
  for (const [content, ordinary, recorded] of [
    ["", 0, 0],
    ["one", 1, 1],
    ["one\n", 2, 1],
    ["one\r\ntwo\r\n", 3, 2],
  ]) {
    assert.equal(countLines(content), ordinary);
    assert.equal(countRecordedLines(content), recorded);
  }
});

test("surface entrypoints expose the exact ordered production policies", () => {
  const policies = [
    [
      desktopPolicy,
      [
        ["src-tauri/src", [".rs"], 1000],
        ["src-tauri/crates", [".rs"], 1000],
        ["src/app", [".ts", ".tsx"], 1000],
        ["src/features", [".ts", ".tsx"], 1000],
        ["src/shared/api", [".ts", ".tsx"], 1000],
        ["src/shared/context", [".ts", ".tsx"], 1000],
        ["src/shared/lib", [".ts", ".tsx"], 1000],
        ["src/shared/ui", [".ts", ".tsx"], 1000],
        ["src/shared/styles", [".css"], 1000],
      ],
    ],
    [mobilePolicy, [["lib", [".dart"], 1000]]],
    [
      webPolicy,
      [
        ["src/app", [".ts", ".tsx"], 1000],
        ["src/features", [".ts", ".tsx"], 1000],
        ["src/shared/api", [".ts", ".tsx"], 1000],
      ],
    ],
  ];

  for (const [policy, expectedRules] of policies) {
    const actualRules = policy.rules.map((rule) => [
      rule.root,
      [...rule.extensions],
      rule.maxLines,
    ]);
    assert.deepEqual(
      actualRules,
      expectedRules,
      `${policy.label} production rules`,
    );

    for (const rule of policy.rules) {
      assert.equal(
        evaluateFileSize({
          baseLines: null,
          candidateLines: rule.maxLines,
          maxLines: rule.maxLines,
        }).violates,
        false,
        `${policy.label} ${rule.root} should allow the ceiling`,
      );
      assert.equal(
        evaluateFileSize({
          baseLines: null,
          candidateLines: rule.maxLines + 1,
          maxLines: rule.maxLines,
        }).violates,
        true,
        `${policy.label} ${rule.root} should reject ceiling + 1`,
      );
    }
  }
});

test("new files use the configured ceiling", () => {
  assert.equal(allowedLineCount(null, 1000), 1000);
  assert.deepEqual(
    evaluateFileSize({ baseLines: null, candidateLines: 1000, maxLines: 1000 }),
    {
      limit: 1000,
      violates: false,
    },
  );
  assert.equal(
    evaluateFileSize({ baseLines: null, candidateLines: 1001, maxLines: 1000 })
      .violates,
    true,
  );
});

test("a compliant file may not cross the ceiling", () => {
  assert.equal(
    evaluateFileSize({ baseLines: 996, candidateLines: 1000, maxLines: 1000 })
      .violates,
    false,
  );
  assert.equal(
    evaluateFileSize({ baseLines: 996, candidateLines: 1003, maxLines: 1000 })
      .violates,
    true,
  );
});

test("parses modifications, deletions, and renames from Git's NUL format", () => {
  assert.deepEqual(
    parseChangedFiles(
      "M\0desktop/src/a.ts\0D\0desktop/src/b.ts\0R100\0desktop/src/old.ts\0desktop/src/new.ts\0",
    ),
    [
      { status: "M", path: "desktop/src/a.ts" },
      { status: "D", path: "desktop/src/b.ts" },
      {
        status: "R",
        oldPath: "desktop/src/old.ts",
        path: "desktop/src/new.ts",
      },
    ],
  );
});

test("an inherited oversized file may hold or shrink but not grow", () => {
  assert.equal(allowedLineCount(1026, 1000), 1026);
  assert.equal(
    evaluateFileSize({ baseLines: 1026, candidateLines: 1026, maxLines: 1000 })
      .violates,
    false,
  );
  assert.equal(
    evaluateFileSize({ baseLines: 1026, candidateLines: 1001, maxLines: 1000 })
      .violates,
    false,
  );
  assert.equal(
    evaluateFileSize({ baseLines: 1026, candidateLines: 1027, maxLines: 1000 })
      .violates,
    true,
  );
});

test("recorded baselines fail growth, shrinkage, and pass unchanged", () => {
  assert.deepEqual(
    evaluateBaselineFileSize({ recordedLines: 1001, candidateLines: 1002 }),
    { violates: true, direction: "grew" },
  );
  assert.deepEqual(
    evaluateBaselineFileSize({ recordedLines: 1001, candidateLines: 1000 }),
    { violates: true, direction: "shrank" },
  );
  assert.deepEqual(
    evaluateBaselineFileSize({ recordedLines: 1001, candidateLines: 1001 }),
    { violates: false, direction: "unchanged" },
  );
});

test("a recorded baseline above 1000 replaces MAX_LINES", () => {
  assert.equal(
    evaluateBaselineFileSize({ recordedLines: 1400, candidateLines: 1400 })
      .violates,
    false,
  );
});

test("rejects malformed file-size baseline manifests", () => {
  assert.throws(
    () =>
      parseFileSizeBaselines(
        JSON.stringify({
          files: {
            "src\\bad.ts": {
              lines: 10,
              reason: "test",
              recordedAt: "2026-08-10",
            },
          },
        }),
      ),
    /not a project-relative POSIX path/,
  );
  assert.throws(
    () =>
      parseFileSizeBaselines(
        JSON.stringify({
          files: {
            "src/good.ts": { lines: 10, reason: "test" },
          },
        }),
      ),
    /must have a non-negative integer lines/,
  );
});

test("a non-listed file keeps the ordinary file-size semantics", () => {
  assert.equal(
    evaluateFileSize({ baseLines: 1001, candidateLines: 1001, maxLines: 1000 })
      .violates,
    false,
  );
  assert.equal(
    evaluateFileSize({ baseLines: 1001, candidateLines: 1002, maxLines: 1000 })
      .violates,
    true,
  );
});

test("runFileSizeCheck loads a project baseline and reports its exact candidate count", async () => {
  const repo = mkdtempSync(path.join(tmpdir(), "file-size-manifest-"));
  const projectRoot = path.join(repo, "desktop");
  mkdirSync(path.join(projectRoot, "scripts"), { recursive: true });
  mkdirSync(path.join(projectRoot, "src"), { recursive: true });
  writeFileSync(
    path.join(projectRoot, "src", "upstream.ts"),
    "one\ntwo\nthree\n",
  );
  writeFileSync(
    path.join(projectRoot, "src", "ordinary.ts"),
    Array.from({ length: 1000 }, (_, index) => `line ${index}`).join("\n"),
  );
  writeFileSync(
    path.join(projectRoot, "scripts", "file-size-baselines.json"),
    JSON.stringify({
      files: {
        "src/upstream.ts": {
          lines: 3,
          reason: "upstream-owned fixture",
          recordedAt: "2026-08-10",
        },
      },
    }),
  );
  git(repo, "init", "-b", "main");
  git(repo, "config", "user.name", "Test");
  git(repo, "config", "user.email", "test@example.com");
  git(repo, "add", "desktop");
  git(repo, "commit", "-m", "base");
  git(repo, "remote", "add", "origin", repo);
  git(repo, "fetch", "origin", "main:refs/remotes/origin/main");
  git(repo, "switch", "-c", "feature");
  const fixtureBase = git(repo, "rev-parse", "HEAD");
  const runFixtureCheck = async (options) => {
    const originalBase = process.env.CHECK_FILE_SIZES_BASE;
    process.env.CHECK_FILE_SIZES_BASE = fixtureBase;
    try {
      return await runFileSizeCheck(options);
    } finally {
      if (originalBase === undefined) {
        delete process.env.CHECK_FILE_SIZES_BASE;
      } else {
        process.env.CHECK_FILE_SIZES_BASE = originalBase;
      }
    }
  };
  writeFileSync(
    path.join(projectRoot, "src", "upstream.ts"),
    readFileSync(path.join(projectRoot, "src", "upstream.ts"), "utf8") +
      "four\n",
  );
  writeFileSync(
    path.join(projectRoot, "src", "ordinary.ts"),
    `${readFileSync(path.join(projectRoot, "src", "ordinary.ts"), "utf8")}\nextra`,
  );

  const errors = [];
  const originalError = console.error;
  const originalExitCode = process.exitCode;
  console.error = (...args) => errors.push(args.join(" "));
  process.exitCode = undefined;
  try {
    await runFixtureCheck({
      projectRoot,
      rules: [
        {
          root: "src",
          extensions: new Set([".ts"]),
          maxLines: 1000,
        },
      ],
      label: "Fixture",
    });
  } finally {
    console.error = originalError;
    process.exitCode = originalExitCode;
  }
  assert.match(errors.join("\n"), /recorded baseline 3 -> 4 lines \(grew\)/);
  assert.match(errors.join("\n"), /update the one lines value/);
  assert.match(
    errors.join("\n"),
    /Keep new files at or below the limit; files already over it may not grow\./,
  );

  writeFileSync(path.join(projectRoot, "src", "upstream.ts"), "one\ntwo\n");
  const shrinkErrors = [];
  const shrinkOriginalError = console.error;
  const shrinkOriginalExitCode = process.exitCode;
  console.error = (...args) => shrinkErrors.push(args.join(" "));
  process.exitCode = undefined;
  try {
    await runFixtureCheck({
      projectRoot,
      rules: [
        {
          root: "src",
          extensions: new Set([".ts"]),
          maxLines: 1000,
        },
      ],
      label: "Fixture",
    });
  } finally {
    console.error = shrinkOriginalError;
    process.exitCode = shrinkOriginalExitCode;
  }
  assert.match(
    shrinkErrors.join("\n"),
    /recorded baseline 3 -> 2 lines \(shrank — tighten the recorded baseline to 2\)/,
  );

  rmSync(path.join(projectRoot, "src", "upstream.ts"));
  await assert.rejects(
    () =>
      runFixtureCheck({
        projectRoot,
        rules: [
          {
            root: "src",
            extensions: new Set([".ts"]),
            maxLines: 1000,
          },
        ],
        label: "Fixture",
      }),
    /Stale file-size baseline entry src\/upstream\.ts: remove it or repoint it at the new path/,
  );
});
