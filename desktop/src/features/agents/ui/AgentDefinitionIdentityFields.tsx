import { AgentIdentityFields } from "./AgentDescriptionField";
import { Textarea } from "@/shared/ui/textarea";
import { cn } from "@/shared/lib/cn";
import {
  PERSONA_FIELD_CONTROL_CLASS,
  PERSONA_FIELD_SHELL_CLASS,
} from "./agentConfigOptions";

export function AgentDefinitionIdentityFields({
  description,
  onDescriptionChange,
  displayName,
  isCreateMode,
  isPending,
  onDisplayNameChange,
  onSystemPromptChange,
  runtimeWarningText,
  showStandaloneRuntimeWarning,
  systemPrompt,
}: {
  description: string;
  onDescriptionChange: (next: string) => void;
  displayName: string;
  isCreateMode: boolean;
  isPending: boolean;
  onDisplayNameChange: (next: string) => void;
  onSystemPromptChange: (next: string) => void;
  runtimeWarningText: string | null;
  showStandaloneRuntimeWarning: boolean;
  systemPrompt: string;
}) {
  return (
    <>
      <AgentIdentityFields
        displayName={displayName}
        onDisplayNameChange={onDisplayNameChange}
        description={description}
        onDescriptionChange={onDescriptionChange}
        disabled={isPending}
        nameRequired={isCreateMode}
      />

      {showStandaloneRuntimeWarning ? (
        <p
          className="rounded-lg border border-warning/40 bg-warning/10 px-3 py-2 text-sm text-warning"
          data-testid="persona-runtime-unavailable"
        >
          {runtimeWarningText} Visit Settings &gt; Agents to set it up. Add
          agent stays disabled until this harness is available.
        </p>
      ) : null}

      <div className="space-y-1.5">
        <label
          className="text-sm font-medium text-foreground"
          htmlFor="persona-system-prompt"
        >
          Agent instructions (optional)
        </label>
        <div className={PERSONA_FIELD_SHELL_CLASS}>
          <Textarea
            className={cn(
              "min-h-40 resize-y px-3 py-3 leading-5",
              PERSONA_FIELD_CONTROL_CLASS,
            )}
            disabled={isPending}
            id="persona-system-prompt"
            onChange={(event) => onSystemPromptChange(event.target.value)}
            placeholder="Describe what this agent should do."
            value={systemPrompt}
          />
        </div>
      </div>
    </>
  );
}
