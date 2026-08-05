import * as DialogPrimitive from "@radix-ui/react-dialog";
import type * as React from "react";

import {
  AuxiliaryPanelBody,
  AuxiliaryPanelHeader,
  AuxiliaryPanelHeaderGroup,
  AuxiliaryPanelTitle,
} from "@/shared/layout/AuxiliaryPanel";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import {
  OverlayPanelBackdrop,
  PANEL_BASE_CLASS,
  PANEL_ENTER_MOTION_CLASS,
  PANEL_OVERLAY_CLASS,
} from "@/shared/ui/OverlayPanelBackdrop";

type ChannelWorktreesDrawerShellProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  isDark: boolean;
  subtitle: string;
  selectedCount: number;
  busy: boolean;
  onRemoveSelected: () => void;
  children: React.ReactNode;
};

export function ChannelWorktreesDrawerShell({
  open,
  onOpenChange,
  isDark,
  subtitle,
  selectedCount,
  busy,
  onRemoveSelected,
  children,
}: ChannelWorktreesDrawerShellProps) {
  return (
    <DialogPrimitive.Root onOpenChange={onOpenChange} open={open}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay asChild>
          <OverlayPanelBackdrop onClose={() => onOpenChange(false)} />
        </DialogPrimitive.Overlay>
      </DialogPrimitive.Portal>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Content
          className={cn(
            PANEL_BASE_CLASS,
            PANEL_OVERLAY_CLASS,
            PANEL_ENTER_MOTION_CLASS,
            "flex w-[400px] cursor-default flex-col overflow-hidden p-0 data-[state=closed]:animate-out data-[state=closed]:slide-out-to-right data-[state=closed]:duration-200",
            isDark
              ? "bg-background/85 backdrop-blur-xl supports-backdrop-filter:bg-background/75"
              : "bg-background",
          )}
          data-testid="channel-worktrees-drawer"
        >
          <AuxiliaryPanelHeader mode="panel">
            <AuxiliaryPanelHeaderGroup mode="panel">
              <AuxiliaryPanelTitle>Worktrees</AuxiliaryPanelTitle>
              <Button
                onClick={() => onOpenChange(false)}
                size="sm"
                type="button"
                variant="ghost"
              >
                Close
              </Button>
            </AuxiliaryPanelHeaderGroup>
            <p className="px-4 pb-2 text-2xs text-muted-foreground">
              {subtitle}
            </p>
          </AuxiliaryPanelHeader>
          <AuxiliaryPanelBody className="space-y-4 px-3 pb-4" mode="panel">
            {children}
          </AuxiliaryPanelBody>
          {selectedCount > 0 ? (
            <div className="flex items-center justify-between border-t border-border/60 px-3 py-2">
              <span className="text-2xs text-muted-foreground">
                {selectedCount} selected
              </span>
              <Button
                disabled={busy}
                onClick={onRemoveSelected}
                size="sm"
                type="button"
                variant="destructive"
              >
                Free local space
              </Button>
            </div>
          ) : null}
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}
