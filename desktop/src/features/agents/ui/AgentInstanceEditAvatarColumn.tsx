import { Button } from "@/shared/ui/button";
import { AgentCreationPreview } from "./AgentCreationPreview";

type AgentInstanceEditAvatarColumnProps = {
  disabled?: boolean;
  label: string;
  avatarUrl: string | null;
  onClearAvatar: () => void;
  onSelectAvatar: (url: string) => void;
  onUploadPendingChange: (pending: boolean) => void;
  onEditLinkedPersona?: () => void;
  onClose: () => void;
  runtimeId?: string | null;
};

export function AgentInstanceEditAvatarColumn({
  disabled = false,
  avatarUrl,
  label,
  onClearAvatar,
  onClose,
  onEditLinkedPersona,
  onSelectAvatar,
  onUploadPendingChange,
  runtimeId = null,
}: AgentInstanceEditAvatarColumnProps) {
  return (
    <div className="flex flex-col items-center gap-2">
      {/* Avatar is definition-level identity. hideEditControl suppresses
          the internal pencil badge; the CTA below is the only edit path. */}
      <AgentCreationPreview
        avatarUrl={avatarUrl}
        hideEditControl
        label={label}
        onClearAvatar={onClearAvatar}
        onUploadPendingChange={onUploadPendingChange}
        onSelectAvatar={onSelectAvatar}
        runtimeId={runtimeId}
      />
      {onEditLinkedPersona ? (
        <Button
          className="w-full"
          disabled={disabled}
          onClick={() => {
            onClose();
            onEditLinkedPersona();
          }}
          size="sm"
          type="button"
          variant="outline"
        >
          Edit avatar
        </Button>
      ) : (
        <p className="text-center text-xs text-muted-foreground">
          Avatar is shared identity
        </p>
      )}
    </div>
  );
}
