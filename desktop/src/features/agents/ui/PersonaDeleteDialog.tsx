import * as React from "react";

import type { AgentPersona } from "@/shared/api/types";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/shared/ui/alert-dialog";
import { Button } from "@/shared/ui/button";
import {
  HermesProfileOffboardFields,
  type HermesProfileOffboardChoice,
} from "./HermesProfileOffboardFields";
import { isNonOwnerOnlyRespondTo } from "./HermesProfileCreateAffordance";

type PersonaDeleteDialogProps = {
  open: boolean;
  persona: AgentPersona | null;
  /** Number of managed-agent instances backed by this persona. Omit or pass 0 to suppress the instance-count sentence. */
  instanceCount?: number;
  /** Unique Hermes profile names bound on cascade-deleted instances. */
  hermesProfiles?: string[];
  runningHermesProfiles?: string[];
  onConfirm: (
    persona: AgentPersona,
    options?: { archiveHermesProfiles?: boolean; hermesProfileReason?: string },
  ) => void;
  onOpenChange: (open: boolean) => void;
};

/**
 * Confirmation copy for deleting a persona. Pure so the cascade archival
 * disclosure stays unit-testable without a renderer: whenever instances are
 * cascade-deleted, each one's identity is also archived on the relay
 * (NIP-IA), and that durable side effect must be disclosed before the
 * destructive confirm — matching the direct agent-delete dialog.
 */
export function personaDeleteDescription(
  persona: AgentPersona | null,
  instanceCount: number,
  hermesProfileCount = 0,
): string {
  if (!persona) {
    return "Delete this agent.";
  }

  const retainedState =
    hermesProfileCount > 0
      ? "Messages, DM history, runtime installations, and worktrees remain in place. Hermes profiles remain on this machine unless you explicitly choose Archive below."
      : "Messages, DM history, runtime installations, and worktrees remain in place. Hermes profile data is untouched.";
  if (instanceCount === 0) {
    return `Remove the ${persona.displayName} persona definition. ${retainedState}`;
  }
  const cascade =
    instanceCount === 1
      ? "Delete 1 linked agent instance and archive its identity on the relay, so it no longer appears in member lists or mention suggestions."
      : `Delete ${instanceCount} linked agent instances and archive their identities on the relay, so they no longer appear in member lists or mention suggestions.`;
  return `Remove the ${persona.displayName} persona definition. ${cascade} ${retainedState}`;
}

export function personaDeleteTitle(persona: AgentPersona | null): string {
  return persona ? `Delete ${persona.displayName}?` : "Delete agent?";
}

export function PersonaDeleteDialog({
  open,
  persona,
  instanceCount = 0,
  hermesProfiles = [],
  runningHermesProfiles = [],
  onConfirm,
  onOpenChange,
}: PersonaDeleteDialogProps) {
  const uniqueProfiles = React.useMemo(
    () =>
      [...new Set(hermesProfiles.map((p) => p.trim()).filter(Boolean))].sort(),
    [hermesProfiles],
  );
  const [profileChoice, setProfileChoice] =
    React.useState<HermesProfileOffboardChoice>("keep");
  const [profileReason, setProfileReason] = React.useState("");

  React.useEffect(() => {
    if (open) {
      setProfileChoice("keep");
      setProfileReason("");
    }
  }, [open]);

  const primaryProfile = uniqueProfiles[0] ?? null;
  const showPublicWarning = isNonOwnerOnlyRespondTo(persona?.respondTo);

  return (
    <AlertDialog onOpenChange={onOpenChange} open={open}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{personaDeleteTitle(persona)}</AlertDialogTitle>
          <AlertDialogDescription>
            {personaDeleteDescription(
              persona,
              instanceCount,
              uniqueProfiles.length,
            )}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {primaryProfile ? (
          <div className="space-y-2">
            <HermesProfileOffboardFields
              choice={profileChoice}
              onChoiceChange={setProfileChoice}
              onReasonChange={setProfileReason}
              profileName={primaryProfile}
              reason={profileReason}
              isRunning={runningHermesProfiles.includes(primaryProfile)}
              showPublicAgentWarning={showPublicWarning}
            />
            {uniqueProfiles.length > 1 ? (
              <p className="text-xs text-muted-foreground">
                Also applies to: {uniqueProfiles.slice(1).join(", ")}
              </p>
            ) : null}
          </div>
        ) : null}
        <AlertDialogFooter>
          <AlertDialogCancel asChild>
            <Button type="button" variant="outline">
              Cancel
            </Button>
          </AlertDialogCancel>
          <AlertDialogAction asChild>
            <Button
              onClick={() => {
                if (persona) {
                  onConfirm(
                    persona,
                    primaryProfile
                      ? {
                          archiveHermesProfiles: profileChoice === "archive",
                          hermesProfileReason: profileReason,
                        }
                      : undefined,
                  );
                }
              }}
              type="button"
              variant="destructive"
            >
              Delete
            </Button>
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
