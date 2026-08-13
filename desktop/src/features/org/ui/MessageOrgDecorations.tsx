import type { ReactNode } from "react";

import { isBudgetStopMessage } from "@/features/org/lib/budgetTag";
import {
  parseHandoffTag,
  parentEventIdFromTags,
} from "@/features/org/lib/handoffTag";
import { HandoffChip } from "@/features/org/ui/HandoffChip";
import type { TimelineMessage } from "@/features/messages/types";
import { useIdentityQuery } from "@/shared/api/hooks";
import { useMyRelayMembershipQuery } from "@/features/community-members/hooks";

export function BudgetStopCard({
  message,
  children,
}: {
  message: TimelineMessage;
  children: ReactNode;
}) {
  const identity = useIdentityQuery();
  const membership = useMyRelayMembershipQuery();
  const isFounder = membership.data?.role === "owner";
  if (!isBudgetStopMessage(message.tags)) {
    return <>{children}</>;
  }
  return (
    <section
      className="rounded-lg border border-destructive/40 bg-destructive/5 p-3"
      data-testid={`budget-stop-${message.id}`}
    >
      <p className="text-sm font-medium">⛔ Budget reached · stop-and-report</p>
      <div className="mt-2 text-sm">{children}</div>
      {isFounder && identity.data?.pubkey ? (
        <a
          className="mt-2 inline-block text-2xs text-primary underline"
          href="#/org"
        >
          Raise budget
        </a>
      ) : null}
    </section>
  );
}

export function ParentKickoffBreadcrumb({
  message,
}: {
  message: TimelineMessage;
}) {
  const parentId = parentEventIdFromTags(message.tags);
  const handoff = parseHandoffTag(message.tags);
  if (!parentId || !handoff) {
    return null;
  }
  return (
    <p className="mb-1 text-2xs text-muted-foreground">
      ↰ part of parent thread
    </p>
  );
}

export function MessageOrgDecorations({
  message,
  children,
}: {
  message: TimelineMessage;
  children: ReactNode;
}) {
  return (
    <div>
      <HandoffChip message={message} />
      <ParentKickoffBreadcrumb message={message} />
      <BudgetStopCard message={message}>{children}</BudgetStopCard>
    </div>
  );
}
