import { access, readFile } from "node:fs/promises";
import path from "node:path";

function markerText(marker) {
  return marker instanceof RegExp ? marker.toString() : marker;
}

function markerMatches(source, marker) {
  if (marker instanceof RegExp) {
    marker.lastIndex = 0;
    return marker.test(source);
  }
  return source.includes(marker);
}

function markerViolation(kind, rel, entry) {
  return {
    kind,
    rel,
    marker: markerText(entry.marker),
    reason: entry.reason,
  };
}

/**
 * Check files that preserve Crew's channel-first information architecture.
 *
 * @param {object} options
 * @param {string} options.projectRoot
 * @param {Array<object>} options.rules
 * @param {string} [options.label]
 * @param {string} [options.scriptPath]
 * @param {boolean} [options.throwOnViolations]
 * @returns {Promise<Array<object>>}
 */
export async function runChannelFirstIaCheck({
  projectRoot,
  rules,
  label = "Desktop",
  scriptPath = "desktop/scripts/check-channel-first-ia.mjs",
  throwOnViolations = true,
}) {
  const violations = [];

  for (const rule of rules) {
    const filePath = path.join(projectRoot, rule.path);
    const exists = await access(filePath)
      .then(() => true)
      .catch(() => false);

    if (!exists) {
      violations.push({
        kind: "missing",
        rel: rule.path,
        marker: "file",
        reason: `guarded file must remain present for the channel-first IA contract (#278)`,
      });
      continue;
    }

    const source = await readFile(filePath, "utf8");

    for (const entry of rule.banned ?? []) {
      if (markerMatches(source, entry.marker)) {
        violations.push(markerViolation("banned", rule.path, entry));
      }
    }

    for (const entry of rule.required ?? []) {
      if (!markerMatches(source, entry.marker)) {
        violations.push(markerViolation("required", rule.path, entry));
      }
    }
  }

  if (violations.length > 0 && throwOnViolations) {
    const body = violations
      .map(
        (item) =>
          `  [${item.kind}] ${item.rel}\n    marker: ${item.marker}\n    reason: ${item.reason}`,
      )
      .join("\n");
    throw new Error(
      `${label} channel-first IA check failed (${violations.length}):\n${body}\n\nFix the guarded file according to ${scriptPath}.`,
    );
  }

  return violations;
}
