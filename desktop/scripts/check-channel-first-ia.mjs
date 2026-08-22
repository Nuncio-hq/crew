import path from "node:path";
import { fileURLToPath } from "node:url";
import { runChannelFirstIaCheck } from "../../scripts/check-channel-first-ia-core.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(__dirname, "..");

const banned = (marker, reason) => ({ marker, reason });
const required = (marker, reason) => ({ marker, reason });

export const CHANNEL_FIRST_IA_RULES = [
  {
    path: "src/features/sidebar/ui/AppSidebar.tsx",
    banned: [
      banned(
        "onSelectProjects",
        "preserve channel-first navigation (#223, D-066)",
      ),
      banned(
        "onSelectWorkbench",
        "preserve channel-first navigation (#219, D-065)",
      ),
      banned(
        "useProjectFolderChannelIds",
        "keep project-folder filtering out of the channel sidebar (#223, D-066)",
      ),
      banned(
        "WorkTreeSection",
        "keep work-tree navigation out of the sidebar (#219, D-065)",
      ),
    ],
  },
  {
    path: "src/features/sidebar/ui/AppSidebarPinnedHeader.tsx",
    banned: [
      banned(
        "open-projects-view",
        "preserve the channel-first pinned navigation (#223, D-066)",
      ),
      banned(
        "open-workbench-view",
        "preserve the channel-first pinned navigation (#219, D-065)",
      ),
      banned(
        "onSelectProjects",
        "preserve channel-first navigation (#223, D-066)",
      ),
      banned(
        "onSelectWorkbench",
        "preserve channel-first navigation (#219, D-065)",
      ),
    ],
  },
  {
    path: "src/features/work-tree/ui/WorkTreeSidebarBlock.tsx",
    banned: [
      banned(
        "WorkTreeSection",
        "keep work-tree navigation out of the sidebar (#219, D-065)",
      ),
    ],
  },
  {
    path: "src/app/AppShell.tsx",
    banned: [
      banned(
        "onSelectProjects",
        "preserve channel-first navigation (#223, D-066)",
      ),
      banned(
        "onSelectWorkbench",
        "preserve channel-first navigation (#219, D-065)",
      ),
    ],
  },
  {
    path: "src/app/AppShell.helpers.ts",
    banned: [
      banned(
        /selectedView\s*:\s*["']workbench["']/,
        "keep /workbench mapped to Inbox or a channel session (#219, D-065)",
      ),
    ],
  },
  {
    path: "src/app/navigation/useAppNavigation.ts",
    banned: [
      banned(
        /\bto\s*:\s*["']\/workbench["']/,
        "navigate /workbench to Inbox or a channel session, not a picker (#219, D-065)",
      ),
    ],
  },
  {
    path: "src/app/routes/workbench.tsx",
    required: [
      required(
        "redirect",
        "keep the /workbench route redirect-only (#219, D-065)",
      ),
    ],
    banned: [
      banned(
        "WorkbenchScreen",
        "do not restore a Workbench place screen (#219, D-065)",
      ),
      banned(
        "WorkbenchRail",
        "do not restore Workbench rail chrome (#219, D-065)",
      ),
    ],
  },
  {
    path: "src/app/routes/workbench.$channelId.$threadRootId.tsx",
    required: [
      required(
        "redirect",
        "keep the workbench thread route redirect-only (#219, D-065)",
      ),
    ],
    banned: [
      banned(
        "WorkbenchScreen",
        "do not restore a Workbench place screen (#219, D-065)",
      ),
      banned(
        "WorkbenchRail",
        "do not restore Workbench rail chrome (#219, D-065)",
      ),
    ],
  },
  {
    path: "src/features/messages/ui/message-thread-panel-head.tsx",
    required: [
      required("LiveJobDesk", "preserve the live-job desk mount (#219, D-065)"),
    ],
    banned: [
      banned(
        "Open workbench",
        'do not restore an "Open workbench" action (#219, D-065)',
      ),
      banned(
        "WorkbenchScreen",
        "do not restore a Workbench place screen (#219, D-065)",
      ),
    ],
  },
  {
    path: "src/features/channels/ui/ChannelPane.tsx",
    banned: [
      banned(
        "onSelectProjects",
        "preserve channel-first navigation (#223, D-066)",
      ),
      banned(
        "ProjectsOverviewPanel",
        "do not replace the Crew channel pane with Projects (#223, D-066)",
      ),
    ],
  },
  {
    path: "playwright.config.ts",
    required: [
      required(
        "channels-only-sidebar.spec.ts",
        "an upstream merge that reorders/replaces the Playwright test-match globs must not silently drop the channel-first IA regression specs (#278 Phase 1)",
      ),
      required(
        "sidebar-snapshot.spec.ts",
        "an upstream merge that reorders/replaces the Playwright test-match globs must not silently drop the channel-first IA regression specs (#278 Phase 1)",
      ),
      required(
        "workbench.spec.ts",
        "an upstream merge that reorders/replaces the Playwright test-match globs must not silently drop the channel-first IA regression specs (#278 Phase 1)",
      ),
      required(
        "workspace-binding-selector.spec.ts",
        "an upstream merge that reorders/replaces the Playwright test-match globs must not silently drop the channel-first IA regression specs (#278 Phase 1)",
      ),
    ],
  },
];

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  await runChannelFirstIaCheck({
    projectRoot,
    rules: CHANNEL_FIRST_IA_RULES,
    label: "Desktop",
    scriptPath: "desktop/scripts/check-channel-first-ia.mjs",
  });
}
