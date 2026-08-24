/**
 * Split an evidence message body into structured claim lines, actionable
 * links, and leftover narrative — so the card can render stats/actions
 * instead of dumping raw "failed"/"passed" substrings and opaque URLs.
 */

export type EvidenceLinkKind = "github-pr" | "buzz-pr" | "other";

export type EvidenceBodyLink = {
  href: string;
  kind: EvidenceLinkKind;
  /** Short label for the action button. */
  label: string;
};

export type EvidenceBodyParts = {
  /** Lines starting with Tests: / Diff: / Files: (kept out of narrative). */
  claimLines: string[];
  links: EvidenceBodyLink[];
  /** Prose with claim lines and promoted URLs removed. */
  narrative: string;
};

const CLAIM_LINE_RE = /^(Tests:|Diff:|Files:)/i;
const URL_RE = /https?:\/\/[^\s)>\]]+|buzz:\/\/[^\s)>\]]+/gi;

function classifyLink(href: string): EvidenceBodyLink {
  const trimmed = href.replace(/[),.;]+$/, "");
  if (/^buzz:\/\/pr\b/i.test(trimmed)) {
    return {
      href: trimmed,
      kind: "buzz-pr",
      label: "Open PR in Crew",
    };
  }
  if (/github\.com\/[^/]+\/[^/]+\/pull\/\d+/i.test(trimmed)) {
    return {
      href: trimmed,
      kind: "github-pr",
      label: "Open PR on GitHub",
    };
  }
  return {
    href: trimmed,
    kind: "other",
    label: "Open link",
  };
}

function normalizeWhitespace(value: string): string {
  return value
    .replace(/[ \t]+\n/g, "\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

function isMarkdownMediaDestination(line: string, urlStart: number): boolean {
  return /!\[[^\]]*\]\(\s*<?$/.test(line.slice(0, urlStart));
}

/**
 * Pull structured claim lines and URLs out of an evidence body.
 * Safe on empty/malformed input; never throws.
 */
export function splitEvidenceBody(body: string): EvidenceBodyParts {
  if (typeof body !== "string" || body.length === 0) {
    return { claimLines: [], links: [], narrative: "" };
  }

  const claimLines: string[] = [];
  const linkMap = new Map<string, EvidenceBodyLink>();
  const narrativeLines: string[] = [];

  for (const rawLine of body.split(/\r?\n/)) {
    const line = rawLine.trimEnd();
    const trimmed = line.trim();
    if (CLAIM_LINE_RE.test(trimmed)) {
      claimLines.push(trimmed);
      continue;
    }

    const narrativeSegments: string[] = [];
    let segmentStart = 0;
    for (const match of line.matchAll(URL_RE)) {
      const raw = match[0];
      narrativeSegments.push(line.slice(segmentStart, match.index));
      if (isMarkdownMediaDestination(line, match.index)) {
        narrativeSegments.push(raw);
      } else {
        const link = classifyLink(raw);
        if (!linkMap.has(link.href)) linkMap.set(link.href, link);
      }
      segmentStart = match.index + raw.length;
    }
    narrativeSegments.push(line.slice(segmentStart));
    const scrubbed = narrativeSegments.join("");

    // Drop bullet leftovers like "- GitHub: " or "- Buzz PR: " once the URL
    // was promoted to an action button.
    const cleaned = scrubbed
      .replace(/^\s*[-*]\s*(?:GitHub|Buzz\s*PR|PR|Link)\s*:\s*$/i, "")
      .replace(/^\s*[-*]\s*$/, "")
      .trimEnd();

    if (cleaned.trim().length > 0) narrativeLines.push(cleaned);
    else if (scrubbed !== line && narrativeLines.length > 0) {
      // Preserve paragraph breaks after a link-only bullet.
      narrativeLines.push("");
    }
  }

  return {
    claimLines,
    links: [...linkMap.values()],
    narrative: normalizeWhitespace(narrativeLines.join("\n")),
  };
}
