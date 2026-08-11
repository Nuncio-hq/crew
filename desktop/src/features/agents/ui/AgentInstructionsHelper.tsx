export function AgentInstructionsHelper({
  hasPersona,
}: {
  hasPersona: boolean;
}) {
  return (
    <div className="space-y-0.5 text-xs text-muted-foreground">
      {hasPersona ? (
        <p>L1: the profile’s SOUL.md — who the person is.</p>
      ) : null}
      <p>L2: the harness base prompt — office rules.</p>
      <p>
        L3: this box — job context for this Crew agent only, appended when
        non-empty.
      </p>
    </div>
  );
}
