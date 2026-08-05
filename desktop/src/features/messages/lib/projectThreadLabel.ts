const LINK_REFERENCE_LINE = /^\[[^\]]+\]:\s*</;
/**
 * Leading @mentions: first token after @, then optional Capitalized name parts.
 * Trailing whitespace is optional so a mentions-only body still strips cleanly.
 */
const LEADING_MENTION =
  /^@[\p{L}][\p{L}0-9'._-]*(?:\s+[\p{Lu}][\p{L}0-9'._-]*)*(?:\s+|$)/u;

const MAX_LABEL_CHARS = 40;

/**
 * Derive a short display label from a project-thread root body.
 * Pure — no I/O. Returns null when nothing usable remains.
 */
export function projectThreadLabel(
  body: string | null | undefined,
): string | null {
  if (!body) return null;
  const normalized = body.replace(/\r\n/g, "\n");
  const lines = normalized.split("\n");
  let index = 0;
  while (
    index < lines.length &&
    LINK_REFERENCE_LINE.test(lines[index].trim())
  ) {
    index += 1;
  }
  while (index < lines.length && lines[index].trim() === "") {
    index += 1;
  }
  let text = lines.slice(index).join("\n").trim();
  if (!text) return null;

  let stripped = text;
  for (;;) {
    const next = stripped.replace(LEADING_MENTION, "");
    if (next === stripped) break;
    stripped = next.trimStart();
  }
  text = stripped.replace(/\s+/g, " ").trim();
  if (!text) return null;

  // Drop pure media/system placeholders like "[screenshot]".
  if (/^\[[^\]]+\]$/.test(text)) return null;

  return truncateOnWordBoundary(text, MAX_LABEL_CHARS);
}

function truncateOnWordBoundary(text: string, max: number): string {
  if (text.length <= max) return text;
  const slice = text.slice(0, max);
  const lastSpace = slice.lastIndexOf(" ");
  const cut = lastSpace > max * 0.5 ? slice.slice(0, lastSpace) : slice;
  return `${cut.trimEnd()}…`;
}
