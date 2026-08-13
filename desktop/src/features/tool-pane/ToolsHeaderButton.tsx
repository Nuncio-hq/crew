import { Wrench } from "lucide-react";

import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";

import { openToolPane, toggleToolPane, useToolPane } from "./toolPaneStore";

export function ToolsHeaderButton() {
  const pane = useToolPane();
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          aria-label={pane.open ? "Hide Tools" : "Open Tools"}
          data-testid="tools-header-button"
          size="icon"
          title="Tools"
          type="button"
          variant={pane.open ? "secondary" : "outline"}
        >
          <Wrench />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem
          data-testid="tools-open-sim"
          onSelect={() => toggleToolPane("sim")}
        >
          Simulator
          <span className="ml-auto text-2xs text-muted-foreground">⇧⌘M</span>
        </DropdownMenuItem>
        <DropdownMenuItem
          data-testid="tools-open-browser"
          onSelect={() => openToolPane("browser")}
        >
          Browser
          <span className="ml-auto text-2xs text-muted-foreground">⇧⌘B</span>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
