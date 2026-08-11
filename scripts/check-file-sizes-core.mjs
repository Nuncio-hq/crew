import { execFileSync } from "node:child_process";
import { promises as fs } from "node:fs";
import path from "node:path";

function git(args, cwd, options = {}) {
  return execFileSync("git", args, {
    cwd,
    encoding: "utf8",
    maxBuffer: 10 * 1024 * 1024,
    ...options,
  });
}

function toPosixPath(relativePath) {
  return relativePath.split(path.sep).join("/");
}

export function countLines(content) {
  if (content.length === 0) {
    return 0;
  }
  return content.split(/\r?\n/).length;
}

// Recorded baselines intentionally use wc -l semantics so manifest values
// match the line counts reviewers see in issue reports and shell commands.
export function countRecordedLines(content) {
  return content.endsWith("\n") ? countLines(content) - 1 : countLines(content);
}

export function allowedLineCount(baseLines, maxLines) {
  return baseLines == null || baseLines <= maxLines ? maxLines : baseLines;
}

export function evaluateFileSize({ baseLines, candidateLines, maxLines }) {
  const limit = allowedLineCount(baseLines, maxLines);
  return { limit, violates: candidateLines > limit };
}

export function evaluateBaselineFileSize({ recordedLines, candidateLines }) {
  if (candidateLines === recordedLines) {
    return { violates: false, direction: "unchanged" };
  }

  return {
    violates: true,
    direction: candidateLines > recordedLines ? "grew" : "shrank",
  };
}

export function parseFileSizeBaselines(
  content,
  manifestPath = "file-size-baselines.json",
) {
  let manifest;
  try {
    manifest = JSON.parse(content);
  } catch (error) {
    throw new Error(
      `Invalid file-size baseline manifest ${manifestPath}: invalid JSON`,
      {
        cause: error,
      },
    );
  }

  if (
    manifest === null ||
    typeof manifest !== "object" ||
    Array.isArray(manifest) ||
    manifest.files === null ||
    typeof manifest.files !== "object" ||
    Array.isArray(manifest.files)
  ) {
    throw new Error(
      `Invalid file-size baseline manifest ${manifestPath}: expected a top-level object with a files object`,
    );
  }

  const baselines = new Map();
  for (const [relativePath, entry] of Object.entries(manifest.files)) {
    if (
      relativePath.length === 0 ||
      relativePath.startsWith("/") ||
      relativePath.includes("\\") ||
      path.posix.normalize(relativePath) !== relativePath ||
      relativePath.split("/").some((part) => part === ".." || part === ".")
    ) {
      throw new Error(
        `Invalid file-size baseline manifest ${manifestPath}: ${relativePath} is not a project-relative POSIX path`,
      );
    }
    if (
      entry === null ||
      typeof entry !== "object" ||
      Array.isArray(entry) ||
      !Number.isInteger(entry.lines) ||
      entry.lines < 0 ||
      typeof entry.reason !== "string" ||
      entry.reason.length === 0 ||
      typeof entry.recordedAt !== "string" ||
      !/^\d{4}-\d{2}-\d{2}$/.test(entry.recordedAt)
    ) {
      throw new Error(
        `Invalid file-size baseline manifest ${manifestPath}: ${relativePath} must have a non-negative integer lines, a non-empty reason, and recordedAt in YYYY-MM-DD format`,
      );
    }
    baselines.set(relativePath, entry);
  }
  return baselines;
}

function findRule(rules, relativePath) {
  return rules.find((rule) => relativePath.startsWith(`${rule.root}/`));
}

export function resolveBaseRef(repoRoot, env = process.env) {
  if (env.CHECK_FILE_SIZES_BASE) {
    return env.CHECK_FILE_SIZES_BASE;
  }

  if (env.GITHUB_ACTIONS === "true") {
    return "HEAD^1";
  }

  try {
    const mergeBase = git(
      ["merge-base", "origin/main", "HEAD"],
      repoRoot,
    ).trim();
    const head = git(["rev-parse", "HEAD"], repoRoot).trim();
    return mergeBase === head ? "HEAD" : mergeBase;
  } catch (error) {
    throw new Error(
      "Could not resolve the file-size base from origin/main. Fetch origin/main or set CHECK_FILE_SIZES_BASE to an explicit commit.",
      { cause: error },
    );
  }
}

export function parseChangedFiles(output) {
  const fields = output.split("\0");
  const changes = [];

  for (let index = 0; index < fields.length - 1; ) {
    const status = fields[index++];
    if (status.startsWith("R") || status.startsWith("C")) {
      changes.push({
        status: status[0],
        oldPath: fields[index++],
        path: fields[index++],
      });
    } else {
      changes.push({ status: status[0], path: fields[index++] });
    }
  }

  return changes;
}

function changedProjectFiles({ repoRoot, projectRelative, baseRef }) {
  const output = git(
    ["diff", "--name-status", "-z", "-M", baseRef, "--", projectRelative],
    repoRoot,
  );
  const changes = parseChangedFiles(output);
  const trackedPaths = new Set(changes.map((change) => change.path));
  const untracked = git(
    ["ls-files", "--others", "--exclude-standard", "-z", "--", projectRelative],
    repoRoot,
  )
    .split("\0")
    .filter(Boolean);

  for (const filePath of untracked) {
    if (!trackedPaths.has(filePath)) {
      changes.push({ status: "A", path: filePath });
    }
  }
  return changes;
}

function readBaseFile(repoRoot, baseRef, filePath) {
  return git(["show", `${baseRef}:${filePath}`], repoRoot, {
    encoding: null,
  }).toString("utf8");
}

export async function runFileSizeCheck({ projectRoot, rules, label }) {
  // Every governed project is a direct child of the repository root. Derive
  // these paths without Git so hook-provided repository environment variables
  // cannot collapse the project pathspec to an empty string.
  const repoRoot = path.dirname(projectRoot);
  const projectRelative = toPosixPath(path.basename(projectRoot));
  const baseRef = resolveBaseRef(repoRoot);

  // Fail clearly instead of silently turning a missing/shallow base into a pass.
  git(["cat-file", "-e", `${baseRef}^{commit}`], repoRoot);

  const manifestPath = path.join(
    projectRoot,
    "scripts",
    "file-size-baselines.json",
  );
  let baselines = new Map();
  try {
    baselines = parseFileSizeBaselines(
      await fs.readFile(manifestPath, "utf8"),
      toPosixPath(path.relative(repoRoot, manifestPath)),
    );
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }

  for (const relativePath of baselines.keys()) {
    const candidatePath = path.join(projectRoot, relativePath);
    try {
      const stats = await fs.stat(candidatePath);
      if (!stats.isFile()) throw new Error("not a file");
    } catch (error) {
      throw new Error(
        `Stale file-size baseline entry ${relativePath}: remove it or repoint it at the new path`,
        { cause: error },
      );
    }
  }

  const violations = [];
  for (const change of changedProjectFiles({
    repoRoot,
    projectRelative,
    baseRef,
  })) {
    if (change.status === "D") continue;

    const relativePath = toPosixPath(
      path.relative(projectRelative, change.path),
    );
    const baseline = baselines.get(relativePath);
    if (baseline) {
      const candidatePath = path.join(repoRoot, change.path);
      const candidateLines = countRecordedLines(
        await fs.readFile(candidatePath, "utf8"),
      );
      const result = evaluateBaselineFileSize({
        recordedLines: baseline.lines,
        candidateLines,
      });
      if (result.violates) {
        violations.push({
          relativePath,
          baseLines: null,
          candidateLines,
          limit: baseline.lines,
          baseline: true,
          direction: result.direction,
        });
      }
      continue;
    }

    const rule = findRule(rules, relativePath);
    if (!rule?.extensions.has(path.extname(relativePath))) continue;

    const candidatePath = path.join(repoRoot, change.path);
    const candidateLines = countLines(await fs.readFile(candidatePath, "utf8"));
    const basePath = change.oldPath ?? change.path;
    const baseContent =
      change.status === "A" ? null : readBaseFile(repoRoot, baseRef, basePath);
    const baseLines = baseContent == null ? null : countLines(baseContent);
    const result = evaluateFileSize({
      baseLines,
      candidateLines,
      maxLines: rule.maxLines,
    });

    if (result.violates) {
      violations.push({
        relativePath,
        baseLines,
        candidateLines,
        limit: result.limit,
        baseline: false,
      });
    }
  }

  if (violations.length === 0) return;

  console.error(`${label} file size ratchet failed (base ${baseRef}):`);
  for (const violation of violations) {
    const before = violation.baseLines == null ? "new" : violation.baseLines;
    const delta =
      violation.baseLines == null
        ? ""
        : ` (${violation.candidateLines - violation.baseLines >= 0 ? "+" : ""}${violation.candidateLines - violation.baseLines})`;
    console.error(
      violation.baseline
        ? violation.direction === "grew"
          ? `- ${violation.relativePath}: recorded baseline ${violation.limit} -> ${violation.candidateLines} lines (grew)`
          : `- ${violation.relativePath}: recorded baseline ${violation.limit} -> ${violation.candidateLines} lines (shrank — tighten the recorded baseline to ${violation.candidateLines})`
        : `- ${violation.relativePath}: ${before} -> ${violation.candidateLines}${delta} lines (allowed ${violation.limit})`,
    );
  }
  if (violations.some((violation) => violation.baseline)) {
    console.error(
      "For upstream-owned growth, update the one lines value in the project's file-size-baselines.json to the new count in the same upstream sync PR; do not raise MAX_LINES or restructure upstream code. For Crew-owned growth, D-022 applies: extract Crew's additions.",
    );
  }
  if (violations.some((violation) => !violation.baseline)) {
    console.error(
      "Keep new files at or below the limit; files already over it may not grow.",
    );
  }
  process.exitCode = 1;
}
