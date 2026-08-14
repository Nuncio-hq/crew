import { promises as fs } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(__dirname, "..");
const srcRoot = path.join(projectRoot, "src");

/**
 * Color-is-information guard (#204): hex / numeric hsl() / rgb() literals
 * belong in the token sheets (`theme.css`, `crew-theme.css`) or allowlisted
 * decorative cases (avatars, logos, canvas, QR). Components must use
 * semantic tokens (`text-success`, `bg-attention`, `hsl(var(--…))`).
 *
 * Does not match `hsl(var(--token))` or GitHub issue numbers (`#204`).
 */

const HEX_RE = /#(?:[0-9a-fA-F]{8}|[0-9a-fA-F]{6}|[0-9a-fA-F]{3})\b/g;
const HSL_NUMERIC_RE = /\bhsla?\(\s*\d/g;
const RGB_NUMERIC_RE = /\brgba?\(\s*\d/g;

const ALLOWED_PREFIXES = [
  "shared/styles/globals/",
  "shared/theme/",
  "shared/ui/buzz-logo/",
  "testing/",
  "features/profile/ui/ProfileAvatar",
  "features/profile/ui/AnimatedAvatar",
  "features/profile/ui/AvatarCustom",
  "features/profile/lib/animatedAvatar",
  "features/onboarding/ui/LandingBees",
  "features/communities/ui/HostedCommunity",
  "features/messages/ui/DiffViewer.css",
  "features/messages/ui/ComposerImageEditor",
  "features/terminal/",
];

const ALLOWED_FILES = new Set([
  "features/agents/ui/AgentCreationPreview.tsx",
  "features/agents/ui/ManagedAgentLogPanel.tsx",
  "features/agents/ui/TeamIdentityCard.tsx",
  "features/agents/ui/agentConfigControls.tsx",
  "features/channels/ui/ChannelScreenHeader.tsx",
  "features/messages/ui/DirectMessageIntroAvatarStack.tsx",
  "features/messages/ui/MessageThreadSummaryRow.tsx",
  "features/messages/ui/SystemMessageAvatars.tsx",
  "features/onboarding/ui/InviteRedeemForm.tsx",
  "features/onboarding/ui/RuntimeIcon.tsx",
  "features/onboarding/ui/SetupStep.tsx",
  "features/profile/ui/NostrBindConsentDialog.tsx",
  "features/settings/ui/AppearanceSettingsControls.tsx",
  "shared/ui/EmojiBurstProvider.tsx",
  "shared/ui/SpoilerParticles.tsx",
  "shared/ui/markdown.tsx",
  "shared/ui/markdown/MarkdownMermaid.tsx",
  "shared/ui/styled-qr-code.tsx",
  "shared/ui/VideoReviewTimecodeButton.tsx",
  "shared/ui/popoverSurface.ts",
]);

const EXTENSIONS = new Set([".ts", ".tsx", ".css", ".js", ".mjs"]);

async function walk(directory) {
  const entries = await fs.readdir(directory, { withFileTypes: true });
  const files = await Promise.all(
    entries.map(async (entry) => {
      const fullPath = path.join(directory, entry.name);
      if (entry.isDirectory()) return walk(fullPath);
      return [fullPath];
    }),
  );
  return files.flat();
}

function isCommentLine(line) {
  const trimmed = line.trim();
  return (
    trimmed.startsWith("//") ||
    trimmed.startsWith("*") ||
    trimmed.startsWith("/*") ||
    trimmed.startsWith("{/*")
  );
}

function isAllowed(rel) {
  if (rel.endsWith(".test.mjs") || rel.endsWith(".test.ts")) return true;
  if (ALLOWED_FILES.has(rel)) return true;
  return ALLOWED_PREFIXES.some((prefix) => rel.startsWith(prefix));
}

const files = await walk(srcRoot);
const violations = [];

for (const filePath of files) {
  const ext = path.extname(filePath);
  if (!EXTENSIONS.has(ext)) continue;
  const rel = path.relative(srcRoot, filePath).split(path.sep).join("/");
  if (isAllowed(rel)) continue;
  const source = await fs.readFile(filePath, "utf8");
  const lines = source.split("\n");
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (isCommentLine(line)) continue;
    HEX_RE.lastIndex = 0;
    HSL_NUMERIC_RE.lastIndex = 0;
    RGB_NUMERIC_RE.lastIndex = 0;
    const matches = [
      ...(line.match(HEX_RE) ?? []),
      ...(line.match(HSL_NUMERIC_RE) ?? []),
      ...(line.match(RGB_NUMERIC_RE) ?? []),
    ];
    for (const match of matches) {
      violations.push(`${rel}:${i + 1}:${match}`);
    }
  }
}

if (violations.length > 0) {
  console.error(
    "Literal color guard failed. Use semantic tokens (or add an allowlist entry with a reason):\n",
  );
  for (const row of violations) {
    console.error(`  ${row}`);
  }
  console.error("\nSee desktop/scripts/check-literal-colors.mjs");
  process.exit(1);
}

console.log("Literal color guard passed.");
