export function AgentInstructionsHelper({
  hasPersona,
}: {
  hasPersona: boolean;
}) {
  return (
    <div className="space-y-0.5 text-xs text-muted-foreground">
      {hasPersona ? (
        <p>
          The profile’s shared persona (SOUL.md), Hermes-owned; Crew edits it
          write-through.
        </p>
      ) : null}
      <p>
        The harness’s built-in office rules (base_prompt.md) are harness-owned.
      </p>
      <p>
        Instructions for this Crew agent only are added when you fill in Agent
        instructions.
      </p>
    </div>
  );
}
