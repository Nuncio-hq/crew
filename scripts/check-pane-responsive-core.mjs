import { promises as fs } from "node:fs";
import path from "node:path";

/**
 * Heuristic guard for #205 pane-responsive contracts.
 *
 * P1: a `flex-1` className on a text-bearing row without `min-w-0` /
 * `overflow-hidden` / `truncate` is the letter-soup precursor.
 * P3: viewport `sm:`/`md:`/`lg:`/`xl:`/`2xl:` inside pane components —
 * panes resize independently of the window, so those breakpoints are
 * the wrong tool. Window-level screens stay allowlisted.
 *
 * False-positive policy (spike 0051): P1 ignores `flex-col` layout shells
 * that already carry `min-h-0` and no text-row hints (`items-center` +
 * `gap-` without `flex-col` is the risky shape). Remaining hits go on
 * the allowlist as `relativePath:literal`.
 */

const VIEWPORT_BP_RE = /\b(?:sm|md|lg|xl|2xl):(?!any\b)[a-zA-Z\[!]/g;
const FLEX_1_RE = /\bflex-1\b/;
const P1_SAFE_RE =
  /\b(?:min-w-0|overflow-hidden|overflow-x-hidden|overflow-x-auto|overflow-y-auto|overflow-y-hidden|overflow-auto|truncate)\b/;
const FLEX_COL_RE = /\bflex-col\b/;

export const DEFAULT_PANE_ROOTS = [
  "src/features/messages/ui",
  "src/features/channels/ui",
  "src/features/sidebar/ui",
  "src/features/forum/ui",
  "src/features/workbench/ui",
  "src/features/work-tree/ui",
  "src/features/tool-pane",
  "src/features/wiki/ui",
];

/** Window-level / overlay files where viewport breakpoints are legitimate. */
export const DEFAULT_P3_PATH_ALLOWLIST = [
  "src/features/sidebar/ui/AppSidebar.tsx",
  "src/features/wiki/ui/WikiLibraryScreen.tsx",
  "src/features/channels/ui/ChannelBrowserDialog.tsx",
  "src/features/channels/ui/ChannelManagementSheet.tsx",
  "src/features/channels/ui/ChannelManagementSheetRows.tsx",
];

async function walkFiles(directory) {
  const entries = await fs.readdir(directory, { withFileTypes: true });
  const files = await Promise.all(
    entries.map(async (entry) => {
      const fullPath = path.join(directory, entry.name);
      if (entry.isDirectory()) {
        return walkFiles(fullPath);
      }
      return [fullPath];
    }),
  );
  return files.flat();
}

function posixRel(projectRoot, filePath) {
  return path.relative(projectRoot, filePath).split(path.sep).join("/");
}

function classNameCandidates(source) {
  const hits = [];
  const re = /className\s*=\s*(?:\{(?:cn|clsx)\()?["'`]([^"'`]*?)["'`]/g;
  let match = re.exec(source);
  while (match) {
    hits.push(match[1]);
    match = re.exec(source);
  }
  const cnRe = /\bcn\(\s*["'`]([^"'`]*?)["'`]/g;
  match = cnRe.exec(source);
  while (match) {
    hits.push(match[1]);
    match = cnRe.exec(source);
  }
  return [...new Set(hits)];
}

function isP1Violation(className) {
  if (!FLEX_1_RE.test(className)) return false;
  if (P1_SAFE_RE.test(className)) return false;
  // Vertical layout shells: flex-1 grows height, not a squeezed text column.
  if (FLEX_COL_RE.test(className)) return false;
  return true;
}

function p3Hits(source) {
  const found = [];
  VIEWPORT_BP_RE.lastIndex = 0;
  let match = VIEWPORT_BP_RE.exec(source);
  while (match) {
    found.push(match[0]);
    match = VIEWPORT_BP_RE.exec(source);
  }
  return found;
}

function isP3Allowlisted(rel, p3PathAllowlist) {
  return p3PathAllowlist.some(
    (prefix) => rel === prefix || rel.startsWith(`${prefix}/`),
  );
}

/**
 * @param {object} options
 * @param {string} options.projectRoot
 * @param {string[]} [options.paneRoots]
 * @param {string[]} [options.p3PathAllowlist]
 * @param {Set<string>} [options.p1Overrides] `relativePath:classLiteral`
 * @param {Set<string>} [options.p3Overrides] `relativePath:matchedLiteral`
 * @param {string} [options.label]
 * @param {string} [options.scriptPath]
 * @param {boolean} [options.throwOnViolations]
 */
export async function runPaneResponsiveCheck({
  projectRoot,
  paneRoots = DEFAULT_PANE_ROOTS,
  p3PathAllowlist = DEFAULT_P3_PATH_ALLOWLIST,
  p1Overrides = new Set(),
  p3Overrides = new Set(),
  label = "Desktop",
  scriptPath = "desktop/scripts/check-pane-responsive.mjs",
  throwOnViolations = true,
}) {
  const files = (
    await Promise.all(
      paneRoots.map((root) => {
        const dir = path.join(projectRoot, root);
        return fs
          .access(dir)
          .then(() => walkFiles(dir))
          .catch(() => []);
      }),
    )
  )
    .flat()
    .filter((filePath) => /\.(tsx|ts)$/.test(filePath));

  const violations = [];

  for (const filePath of files) {
    const rel = posixRel(projectRoot, filePath);
    const source = await fs.readFile(filePath, "utf8");

    for (const className of classNameCandidates(source)) {
      if (!isP1Violation(className)) continue;
      const key = `${rel}:${className}`;
      if (p1Overrides.has(key)) continue;
      violations.push({
        pattern: "P1",
        rel,
        detail: `flex-1 without min-w-0/truncate/overflow-hidden: ${className}`,
      });
    }

    if (!isP3Allowlisted(rel, p3PathAllowlist)) {
      for (const hit of p3Hits(source)) {
        const key = `${rel}:${hit}`;
        if (p3Overrides.has(key)) continue;
        violations.push({
          pattern: "P3",
          rel,
          detail: `viewport breakpoint in pane component: ${hit}`,
        });
      }
    }
  }

  if (violations.length > 0 && throwOnViolations) {
    const body = violations
      .map((item) => `  [${item.pattern}] ${item.rel}\n    ${item.detail}`)
      .join("\n");
    throw new Error(
      `${label} pane-responsive check failed (${violations.length}):\n${body}\n\nFix: min-w-0 + truncate, or convert sm:/md: to @container. Allowlist in ${scriptPath}.`,
    );
  }

  return violations;
}
