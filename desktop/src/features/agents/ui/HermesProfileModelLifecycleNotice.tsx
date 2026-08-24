/**
 * Explains how Hermes profile-owned models apply — distinct from Crew restart UX.
 */
export function HermesProfileModelLifecycleNotice({
  profileName,
}: {
  profileName: string;
}) {
  const trimmed = profileName.trim();
  const display =
    trimmed === "default" ? "Personal (default)" : trimmed;

  return (
    <p
      className="text-sm text-muted-foreground"
      data-testid="hermes-profile-model-lifecycle-notice"
    >
      Model for profile <strong>{display}</strong> is owned by Hermes (
      <code>~/.hermes</code> or <code>profiles/{trimmed}</code>). Change it in
      Hermes or with <code>hermes config set</code> — not in Crew. Crew will not
      ask you to restart when the profile model changes; the new model applies
      on the next fresh ACP session (use <code>!rotate</code> in Hermes to
      force one).
    </p>
  );
}
