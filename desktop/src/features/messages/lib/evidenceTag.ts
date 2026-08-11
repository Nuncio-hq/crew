const EVIDENCE_KINDS = new Set([
  "test-run",
  "metrics",
  "before-after-visual",
  "diff-stat",
]);

export type EvidenceKind =
  | "test-run"
  | "metrics"
  | "before-after-visual"
  | "diff-stat";

export function parseEvidenceKind(
  tags: readonly (readonly string[])[] | undefined,
): EvidenceKind | null {
  const value = tags?.find((tag) => tag[0] === "crew-evidence")?.[1];
  return value && EVIDENCE_KINDS.has(value) ? (value as EvidenceKind) : null;
}
