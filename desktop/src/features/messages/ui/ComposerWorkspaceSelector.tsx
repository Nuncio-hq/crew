import * as React from "react";

import type { WorkspaceBindingChoice } from "@/features/messages/lib/workspaceBindingSpec";
import { gitProjectWorkspaceForChannel } from "@/features/projects/lib/git-project-channel";
import { useProjectsQuery } from "@/features/projects/hooks";
import { projectBranchOptions } from "@/features/projects/lib/projectBranches";
import { probeProjectGitWorkspace } from "@/shared/api/projectGitWorkspaceProbe";
import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";

export function ComposerWorkspaceSelector({
  channelId,
  onChange,
  value,
}: {
  channelId: string | null;
  onChange: (binding: WorkspaceBindingChoice) => void;
  value: WorkspaceBindingChoice;
}) {
  const projectsQuery = useProjectsQuery();
  const workspace = gitProjectWorkspaceForChannel(
    channelId,
    projectsQuery.data,
  );
  const [probe, setProbe] = React.useState<{
    defaultBranch: string | null;
    currentBranch: string | null;
    localBranches: string[];
    remoteBranches: string[];
  } | null>(null);
  const [open, setOpen] = React.useState(false);
  const [menuShowsBasePicker, setMenuShowsBasePicker] = React.useState(
    value.mode === "new",
  );

  React.useEffect(() => {
    if (!workspace?.localPath) {
      setProbe(null);
      return;
    }
    let cancelled = false;
    void probeProjectGitWorkspace(workspace.localPath)
      .then((result) => {
        if (cancelled || !result.isGit) return;
        setProbe({
          defaultBranch: result.defaultBranch ?? workspace.defaultBranch,
          currentBranch: result.currentBranch,
          localBranches: result.localBranches,
          remoteBranches: result.remoteBranches,
        });
      })
      .catch(() => {
        if (!cancelled) {
          setProbe({
            defaultBranch: workspace.defaultBranch,
            currentBranch: null,
            localBranches: [workspace.defaultBranch].filter(Boolean),
            remoteBranches: [],
          });
        }
      });
    return () => {
      cancelled = true;
    };
  }, [workspace?.defaultBranch, workspace?.localPath]);

  if (!workspace) {
    if (open) setOpen(false);
    return null;
  }

  const defaultBranch =
    probe?.defaultBranch ?? workspace.defaultBranch ?? "main";
  const branches = projectBranchOptions(
    probe?.remoteBranches ?? [],
    probe?.localBranches ?? [defaultBranch],
  );
  const binding = value;
  const handleOpenChange = (next: boolean) => {
    if (next) {
      setMenuShowsBasePicker(binding.mode === "new");
    }
    setOpen(next);
  };
  const applyBinding = (next: WorkspaceBindingChoice) => {
    onChange(next);
    setOpen(false);
  };
  let label: string;
  switch (binding.mode) {
    case "main":
      label = "⌂ Main checkout";
      break;
    case "branch":
      label = `⎇ ${binding.name}`;
      break;
    case "new":
      label = binding.base ? `🌿 ${binding.base}` : "🌿 New worktree";
      break;
    default: {
      const _exhaustive: never = binding;
      label = _exhaustive;
    }
  }

  return (
    <DropdownMenu modal={false} onOpenChange={handleOpenChange} open={open}>
      <DropdownMenuTrigger asChild>
        <Button
          className="max-w-48 truncate text-xs"
          data-testid="composer-workspace-selector"
          size="sm"
          type="button"
          variant="ghost"
        >
          {label}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="end"
        className="w-64"
        onCloseAutoFocus={(event) => event.preventDefault()}
      >
        <DropdownMenuLabel>Where this thread works</DropdownMenuLabel>
        <DropdownMenuRadioGroup
          onValueChange={(next) => {
            if (next === "main") {
              applyBinding({ mode: "main" });
              return;
            }
            if (next === "new") {
              applyBinding({ mode: "new", base: null });
              return;
            }
            if (next.startsWith("branch:")) {
              applyBinding({
                mode: "branch",
                name: next.slice("branch:".length),
              });
            }
          }}
          value={
            binding.mode === "branch" ? `branch:${binding.name}` : binding.mode
          }
        >
          <DropdownMenuRadioItem
            data-testid="composer-workspace-new"
            value="new"
          >
            🌿 New worktree
          </DropdownMenuRadioItem>
          <DropdownMenuRadioItem
            data-testid="composer-workspace-main"
            value="main"
          >
            ⌂ Main checkout
          </DropdownMenuRadioItem>
        </DropdownMenuRadioGroup>
        {menuShowsBasePicker ? (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuLabel>Base branch</DropdownMenuLabel>
            <DropdownMenuRadioGroup
              onValueChange={(next) =>
                applyBinding({
                  mode: "new",
                  base: next === defaultBranch ? null : next,
                })
              }
              value={
                binding.mode === "new" ? (binding.base ?? defaultBranch) : ""
              }
            >
              {branches.map((branch) => (
                <DropdownMenuRadioItem
                  data-testid={`composer-workspace-base-${branch}`}
                  key={`base-${branch}`}
                  value={branch}
                >
                  {branch}
                  {branch === defaultBranch ? " (default)" : ""}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </>
        ) : null}
        <DropdownMenuSeparator />
        <DropdownMenuLabel>Existing branch</DropdownMenuLabel>
        <DropdownMenuRadioGroup
          onValueChange={(next) =>
            applyBinding({ mode: "branch", name: next.slice("branch:".length) })
          }
          value={binding.mode === "branch" ? `branch:${binding.name}` : ""}
        >
          {branches.map((branch) => (
            <DropdownMenuRadioItem
              data-testid={`composer-workspace-branch-${branch}`}
              key={`branch-${branch}`}
              value={`branch:${branch}`}
            >
              ⎇ {branch}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
