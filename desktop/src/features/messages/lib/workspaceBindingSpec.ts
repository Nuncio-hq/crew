export type WorkspaceBindingChoice =
  | { mode: "new"; base: string | null }
  | { mode: "main" }
  | { mode: "branch"; name: string };

export const DEFAULT_WORKSPACE_BINDING: WorkspaceBindingChoice = {
  mode: "new",
  base: null,
};

export type ParsedWorkspaceBinding = {
  ws: "new" | "main" | "branch";
  branch: string | null;
  base: string | null;
};

export function parseWorkspaceBindingParams(
  ws: string | null | undefined,
  base: string | null | undefined,
): ParsedWorkspaceBinding {
  const trimmedWs = ws?.trim() || null;
  const trimmedBase = base?.trim() || null;
  if (trimmedWs === "main") {
    return { ws: "main", branch: null, base: null };
  }
  if (trimmedWs?.startsWith("branch:")) {
    const name = trimmedWs.slice("branch:".length);
    if (name) return { ws: "branch", branch: name, base: null };
  }
  return { ws: "new", branch: null, base: trimmedBase };
}

export function workspaceBindingQuerySuffix(
  binding: WorkspaceBindingChoice,
  defaultBranch: string | null,
): string {
  switch (binding.mode) {
    case "main":
      return "&ws=main";
    case "branch":
      return `&ws=${encodeURIComponent(`branch:${binding.name}`)}`;
    case "new": {
      const base = binding.base?.trim() || null;
      if (base && base !== defaultBranch) {
        return `&base=${encodeURIComponent(base)}`;
      }
      return "";
    }
    default: {
      const _exhaustive: never = binding;
      return _exhaustive;
    }
  }
}

export function isDefaultWorkspaceBinding(
  binding: WorkspaceBindingChoice,
  defaultBranch: string | null,
): boolean {
  return workspaceBindingQuerySuffix(binding, defaultBranch) === "";
}
