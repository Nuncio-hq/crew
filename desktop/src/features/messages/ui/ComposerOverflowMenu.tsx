import { ALargeSmall, AtSign, MoreHorizontal, Paperclip } from "lucide-react";

import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";

/**
 * Composer toolbar overflow (#205). Below 340px container, ingress icons
 * **collapse** into this menu so the send button stays reachable.
 */
export function ComposerOverflowMenu({
  composerDisabled,
  isUploading,
  onCaptureSelection,
  onFormattingToggle,
  onOpenMentionPicker,
  onPaperclip,
}: {
  composerDisabled: boolean;
  isUploading: boolean;
  onCaptureSelection: () => void;
  onFormattingToggle: () => void;
  onOpenMentionPicker: () => void;
  onPaperclip: () => void;
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          aria-label="More composer actions"
          className="hidden shrink-0 [@container(max-width:21.25rem)]:inline-flex"
          data-testid="composer-overflow-menu"
          disabled={composerDisabled}
          onMouseDown={onCaptureSelection}
          size="icon"
          type="button"
          variant="ghost"
        >
          <MoreHorizontal />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="min-w-40">
        <DropdownMenuItem
          disabled={composerDisabled}
          onSelect={onOpenMentionPicker}
        >
          <AtSign />
          Mention
        </DropdownMenuItem>
        <DropdownMenuItem
          disabled={composerDisabled || isUploading}
          onSelect={onPaperclip}
        >
          <Paperclip />
          Attach file
        </DropdownMenuItem>
        <DropdownMenuItem
          disabled={composerDisabled}
          onSelect={onFormattingToggle}
        >
          <ALargeSmall />
          Formatting
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
