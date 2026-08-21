import { Input } from "@/shared/ui/input";
import { Textarea } from "@/shared/ui/textarea";
import { cn } from "@/shared/lib/cn";
import { RequiredFieldLabel } from "./agentConfigControls";
import {
  PERSONA_FIELD_CONTROL_CLASS,
  PERSONA_FIELD_SHELL_CLASS,
} from "./agentConfigOptions";

export function AgentDefinitionIdentityFields({
  displayName,
  isCreateMode,
  isPending,
  onDisplayNameChange,
  onSystemPromptChange,
  runtimeWarningText,
  showStandaloneRuntimeWarning,
  systemPrompt,
}: {
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
      <div className="space-y-1.5">
        <RequiredFieldLabel
          className="text-foreground"
          htmlFor="persona-display-name"
          isRequired={isCreateMode}
        >
          Agent name
        </RequiredFieldLabel>
        <div
          className={cn(
            "flex min-h-11 items-center px-3",
            PERSONA_FIELD_SHELL_CLASS,
          )}
        >
          <Input
            autoCorrect="off"
            className={cn(
              "h-8 px-0 py-0 leading-6",
              PERSONA_FIELD_CONTROL_CLASS,
            )}
            disabled={isPending}
            id="persona-display-name"
            onChange={(event) => onDisplayNameChange(event.target.value)}
            placeholder="Fizz"
            value={displayName}
          />
        </div>
      </div>

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
