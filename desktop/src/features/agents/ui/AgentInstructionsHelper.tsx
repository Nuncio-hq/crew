export function AgentInstructionsHelper({
  hasPersona,
}: {
  hasPersona: boolean;
}) {
  return (
    <div className="space-y-0.5 text-xs text-muted-foreground">
      {hasPersona ? (
        <p>
          L1 SOUL.md = the profile’s persona, Hermes-owned, Crew edits
          write-through.
        </p>
      ) : null}
      <p>L2 base_prompt.md = office rules, harness-owned.</p>
      <p>
        L3 the Crew agent description = optional per-agent job context, appended
        only when non-empty.
      </p>
    </div>
  );
}
