import { BookOpen } from "lucide-react";
import type { Project } from "@/features/projects/projectModels";
import { projectWikiNavigation } from "@/features/projects/lib/projectWikiNavigation";
import { SidebarMenuButton, SidebarMenuItem } from "@/shared/ui/sidebar";
import { SidebarMenuLabel } from "@/shared/ui/sidebar-menu-label";

/** A Project's Wiki child preserves the selected full repository coordinate. */
export function SidebarProjectWikiRow({
  project,
  active,
  onOpen,
}: {
  project: Project;
  active: boolean;
  onOpen: (
    projectId: string,
    search: ReturnType<typeof projectWikiNavigation>,
  ) => unknown;
}) {
  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        aria-label={`${project.name} Wiki`}
        className="h-7 pl-7 text-sidebar-foreground/70"
        data-testid={`sidebar-project-wiki-${project.projectAddress}`}
        isActive={active}
        onClick={() => void onOpen(project.id, projectWikiNavigation(project))}
        type="button"
      >
        <BookOpen aria-hidden="true" className="size-3.5" />
        <SidebarMenuLabel>Wiki</SidebarMenuLabel>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
}
