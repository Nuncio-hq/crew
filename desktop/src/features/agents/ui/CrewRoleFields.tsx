import { Badge } from "@/shared/ui/badge";
import { cn } from "@/shared/lib/cn";
import {
  crewRoleLabel,
  CREW_ROLE_TAXONOMY,
  crewRoleSubmitPatch,
} from "../lib/crewRole";

export { crewRoleSubmitPatch };

export function CrewRoleChip({
  role,
  className,
}: {
  role: string | null | undefined;
  className?: string;
}) {
  if (!role) return null;
  return (
    <Badge
      className={cn("font-normal capitalize", className)}
      data-testid="crew-role-chip"
      variant="secondary"
    >
      {crewRoleLabel(role)}
    </Badge>
  );
}

export function CrewRoleField({
  value,
  onChange,
  disabled,
}: {
  value: string;
  onChange: (next: string) => void;
  disabled?: boolean;
}) {
  return (
    <div className="space-y-1.5" data-testid="crew-role-field">
      <label
        className="text-sm font-medium text-foreground"
        htmlFor="crew-role"
      >
        Crew role
      </label>
      <select
        className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-1 text-sm shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50"
        disabled={disabled}
        id="crew-role"
        onChange={(e) => onChange(e.target.value)}
        value={value}
      >
        <option value="">No role</option>
        {CREW_ROLE_TAXONOMY.map((role) => (
          <option key={role} value={role}>
            {role}
          </option>
        ))}
      </select>
      <p className="text-xs text-muted-foreground">
        Owner-assigned only. Takes effect on the agent&apos;s next fresh session
        (!rotate). Off-role work should be refused in-thread.
      </p>
    </div>
  );
}
