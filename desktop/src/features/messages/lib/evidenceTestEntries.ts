/**
 * Parse named pass/fail rows from an evidence test-run body so the card can
 * expand like Cursor's Checks list instead of only showing "N passed · M failed".
 */

export type EvidenceTestStatus = "passed" | "failed";

export type EvidenceTestEntry = {
  name: string;
  status: EvidenceTestStatus;
};

const SECTION_HEADER_RE =
  /^(#{1,6}\s*)?(failed|failing|passed|passing)\s*:?\s*$/i;
const BULLET_RE = /^\s*(?:[-*]|\d+[.)])\s+(.+?)\s*$/;
const STATUS_PREFIX_RE =
  /^\s*(?:✅|❌|✓|✗|✔|✘|PASS(?:ED)?|FAIL(?:ED)?)\s*[:\-]?\s+(.+?)\s*$/i;
const CLAIM_LINE_RE = /^(Tests:|Diff:|Files:)/i;

function sectionStatus(header: string): EvidenceTestStatus | null {
  const key = header.replace(/^#+\s*/, "").replace(/:$/, "").trim().toLowerCase();
  if (key === "failed" || key === "failing") return "failed";
  if (key === "passed" || key === "passing") return "passed";
  return null;
}

function pushUnique(
  entries: EvidenceTestEntry[],
  seen: Set<string>,
  status: EvidenceTestStatus,
  rawName: string,
): void {
  const name = rawName.replace(/\s+/g, " ").trim();
  if (name.length === 0) return;
  const key = `${status}:${name.toLowerCase()}`;
  if (seen.has(key)) return;
  seen.add(key);
  entries.push({ name, status });
}

/**
 * Extract named test rows from Failed/Passed sections or ✓/✗-prefixed lines.
 * Safe on empty/malformed input; never throws.
 */
export function parseEvidenceTestEntries(body: string): EvidenceTestEntry[] {
  if (typeof body !== "string" || body.length === 0) return [];

  const entries: EvidenceTestEntry[] = [];
  const seen = new Set<string>();
  let section: EvidenceTestStatus | null = null;

  for (const rawLine of body.split(/\r?\n/)) {
    const line = rawLine.trimEnd();
    const trimmed = line.trim();
    if (trimmed.length === 0) {
      section = null;
      continue;
    }
    if (CLAIM_LINE_RE.test(trimmed)) continue;

    const header = trimmed.match(SECTION_HEADER_RE);
    if (header) {
      section = sectionStatus(header[2] ?? "");
      continue;
    }

    const prefixed = trimmed.match(STATUS_PREFIX_RE);
    if (prefixed?.[1]) {
      const token = trimmed.match(/^(✅|❌|✓|✗|✔|✘|PASS(?:ED)?|FAIL(?:ED)?)/i)?.[1];
      const status: EvidenceTestStatus =
        token != null && /^(?:❌|✗|✘|FAIL)/i.test(token) ? "failed" : "passed";
      pushUnique(entries, seen, status, prefixed[1]);
      continue;
    }

    if (section == null) continue;
    const bullet = trimmed.match(BULLET_RE);
    if (bullet?.[1]) {
      pushUnique(entries, seen, section, bullet[1]);
      continue;
    }
    // Plain line under a Failed/Passed heading (agent prose without bullets).
    if (!/^https?:\/\//i.test(trimmed) && !/^buzz:\/\//i.test(trimmed)) {
      pushUnique(entries, seen, section, trimmed);
    }
  }

  return entries;
}

export function groupEvidenceTestEntries(entries: readonly EvidenceTestEntry[]): {
  failed: EvidenceTestEntry[];
  passed: EvidenceTestEntry[];
} {
  const failed: EvidenceTestEntry[] = [];
  const passed: EvidenceTestEntry[] = [];
  for (const entry of entries) {
    if (entry.status === "failed") failed.push(entry);
    else passed.push(entry);
  }
  return { failed, passed };
}
