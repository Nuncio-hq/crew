import { parse as parseYaml, stringify as stringifyYaml } from "yaml";

import type { CanvasTooling } from "./types";

export function parseCanvasTooling(
  content: string | null | undefined,
): CanvasTooling | null {
  if (!content) return null;
  const start = content.indexOf("```crew");
  if (start < 0) return null;
  const bodyStart = content.indexOf("\n", start);
  if (bodyStart < 0) return null;
  const end = content.indexOf("```", bodyStart + 1);
  if (end < 0) return null;
  const yaml = content.slice(bodyStart + 1, end);
  try {
    const doc = parseYaml(yaml) as { tooling?: CanvasTooling } | null;
    if (!doc || typeof doc !== "object" || !doc.tooling) return null;
    return doc.tooling;
  } catch {
    return null;
  }
}

export function writeCanvasTooling(
  content: string,
  tooling: CanvasTooling,
): string {
  const start = content.indexOf("```crew");
  if (start < 0) {
    const yaml = stringifyYaml({
      assignments: {},
      definitions: {},
      tooling,
    });
    return `${content}${content && !content.endsWith("\n") ? "\n" : ""}\n\`\`\`crew\n${yaml}\`\`\`\n`;
  }
  const bodyStart = content.indexOf("\n", start);
  const end = content.indexOf("```", bodyStart + 1);
  if (bodyStart < 0 || end < 0) {
    return content;
  }
  const prefix = content.slice(0, start);
  const suffix = content.slice(end + 3);
  const yaml = content.slice(bodyStart + 1, end);
  const doc = (parseYaml(yaml) as Record<string, unknown> | null) ?? {};
  doc.tooling = tooling;
  return `${prefix}\`\`\`crew\n${stringifyYaml(doc)}\`\`\`${suffix}`;
}

export function toolingHasUdid(tooling: CanvasTooling): boolean {
  const blob = JSON.stringify(tooling).toLowerCase();
  return blob.includes("udid");
}
